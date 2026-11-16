use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const REFERENCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../configs/reference.toml");

fn dir(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn lab(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lab")).args(args).output().unwrap()
}
fn ok(args: &[&str]) -> String {
    let out = lab(args);
    assert!(out.status.success(), "lab {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}
/// Extracts `<hex>` from the line `final tick=<t> hash=<hex>`.
fn final_hash(stdout: &str) -> String {
    let line = stdout.lines().find(|l| l.starts_with("final tick=")).unwrap_or_else(|| panic!("no final line in {stdout:?}"));
    let hash = line.split("hash=").nth(1).unwrap().trim().to_string();
    assert_eq!(hash.len(), 64);
    hash
}
fn run(config: &str, ticks: &str, threads: &str, out: &Path) -> String {
    final_hash(&ok(&["run", config, "--ticks", ticks, "--threads", threads, "--out", out.to_str().unwrap()]))
}

#[test]
fn run_twice_same_hash() {
    let (a, b) = (dir("twice-a"), dir("twice-b"));
    assert_eq!(run(REFERENCE, "100", "2", &a), run(REFERENCE, "100", "2", &b));
}

#[test]
fn threads_do_not_change_hash() {
    let (a, b) = (dir("threads-1"), dir("threads-4"));
    assert_eq!(run(REFERENCE, "200", "1", &a), run(REFERENCE, "200", "4", &b));
}

#[test]
fn resume_equals_straight_run() {
    let (straight, first, second) = (dir("straight"), dir("first"), dir("second"));
    let expected = run(REFERENCE, "200", "2", &straight);
    run(REFERENCE, "100", "2", &first);
    let ckpt = first.join("ckpt-100.bin");
    let resumed = final_hash(&ok(&[
        "resume", ckpt.to_str().unwrap(), "--config", REFERENCE, "--ticks", "100", "--threads", "3", "--out", second.to_str().unwrap(),
    ]));
    assert_eq!(resumed, expected);
    let printed = ok(&["hash", second.join("ckpt-200.bin").to_str().unwrap()]);
    assert_eq!(printed.trim(), format!("tick=200 hash={expected}"));
    let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(second.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["start_tick"], 100);
    assert_eq!(manifest["end_tick"], 200);
}

#[test]
fn checkpoint_every_writes_intermediate_files() {
    let d = dir("periodic");
    let hash = run(REFERENCE, "50", "2", &d);
    let d2 = dir("periodic-2");
    ok(&["run", REFERENCE, "--ticks", "50", "--threads", "2", "--out", d2.to_str().unwrap(), "--checkpoint-every", "20"]);
    for tick in [20, 40, 50] {
        assert!(d2.join(format!("ckpt-{tick}.bin")).exists(), "missing checkpoint at tick {tick}");
    }
    assert!(!d2.join("ckpt-10.bin").exists());
    assert_eq!(ok(&["hash", d2.join("ckpt-50.bin").to_str().unwrap()]).trim(), format!("tick=50 hash={hash}"));
    let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(d2.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["checkpoints"].as_array().unwrap().len(), 3);
}

#[test]
fn resume_rejects_other_config() {
    let (first, second) = (dir("reject-first"), dir("reject-second"));
    run(REFERENCE, "10", "1", &first);
    let altered = first.join("altered.toml");
    std::fs::write(&altered, std::fs::read_to_string(REFERENCE).unwrap().replace("amount = 3", "amount = 4")).unwrap();
    let out = lab(&[
        "resume", first.join("ckpt-10.bin").to_str().unwrap(), "--config", altered.to_str().unwrap(), "--ticks", "5", "--threads", "1", "--out",
        second.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("config hash"), "{stderr}");
    assert!(!second.join("manifest.json").exists(), "a refused resume must not write a manifest");
}

#[test]
fn manifest_complete() {
    let d = dir("manifest");
    let hash = run(REFERENCE, "20", "2", &d);
    let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(d.join("manifest.json")).unwrap()).unwrap();
    let object = manifest.as_object().unwrap();
    for field in [
        "lab_version", "git_commit", "git_dirty", "rustc", "target", "format_version", "config_text", "config_sha256", "seed", "rng", "rules",
        "threads", "start_tick", "end_tick", "checkpoints", "final_sha256", "entity_ticks_per_second",
    ] {
        let value = object.get(field).unwrap_or_else(|| panic!("manifest lacks {field}"));
        assert!(!value.is_null() && value.as_str() != Some(""), "manifest field {field} is empty");
    }
    assert_eq!(manifest["config_text"].as_str().unwrap(), std::fs::read_to_string(REFERENCE).unwrap());
    assert_eq!(manifest["final_sha256"].as_str().unwrap(), hash);
    assert_eq!(manifest["seed"], 20261116);
    assert_eq!(manifest["threads"], 2);
    assert_eq!(manifest["rules"].as_array().unwrap().len(), 5);
    assert_eq!(manifest["rules"][2]["id"], "uptake.fixed");
    assert_eq!(manifest["rules"][2]["stream"], 3);
    assert_eq!(manifest["git_commit"].as_str().unwrap().len(), 40);
}

#[test]
fn too_many_ticks_rejected() {
    let d = dir("too-many");
    let out = lab(&["run", REFERENCE, "--ticks", "4294967296", "--threads", "1", "--out", d.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("tick limit"));
    assert!(!d.join("manifest.json").exists());
}

#[test]
fn bad_config_reports_the_problem() {
    let d = dir("bad-config");
    let bad = d.join("bad.toml");
    std::fs::write(&bad, std::fs::read_to_string(REFERENCE).unwrap().replace("seed = 20261116", "sed = 20261116")).unwrap();
    let out = lab(&["run", bad.to_str().unwrap(), "--ticks", "1", "--threads", "1", "--out", d.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("sed"));
}
