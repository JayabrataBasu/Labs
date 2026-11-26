//! Observation windows and counterfactual forks through the command line.
use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join("observe").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
/// The imposed-specialist world, small.
fn world(dir: &Path) -> PathBuf {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../configs/positive-control.toml")).unwrap();
    let path = dir.join("w.toml");
    std::fs::write(&path, text.replace("width = 48\nheight = 48", "width = 12\nheight = 12").replace("count = 1500", "count = 94")).unwrap();
    path
}
fn lab(args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_lab")).args(args).output().unwrap();
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}
fn ok(args: &[&str]) -> String {
    let (success, text) = lab(args);
    assert!(success, "{args:?}: {text}");
    text
}
fn rows(path: &Path) -> Vec<Vec<String>> {
    std::fs::read_to_string(path).unwrap().lines().skip(1).map(|l| l.split(',').map(str::to_owned).collect()).collect()
}
fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn observation_windows_are_written_and_do_not_change_the_run() {
    let dir = scratch("windows");
    let w = world(&dir);
    let plain = ok(&["run", s(&w), "--ticks", "100", "--threads", "2", "--out", s(&dir.join("a"))]);
    let watched = ok(&["run", s(&w), "--ticks", "100", "--threads", "3", "--out", s(&dir.join("b")), "--observe-window", "20", "--observe-count", "2"]);
    assert_eq!(plain, watched, "same final hash");
    let mut windows: Vec<_> = std::fs::read_dir(dir.join("b/obs")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    windows.sort();
    assert_eq!(windows, ["window-0000000060-0000000080", "window-0000000080-0000000100"]);
    let first = dir.join("b/obs").join(&windows[0]);
    let flows = rows(&first.join("flows.csv"));
    assert!(flows.iter().any(|f| f[2] == "3" && f[4] != "0") && flows.iter().any(|f| f[2] == "3" && f[3] != "0"), "B is released and taken");
    let start = rows(&first.join("entities-start.csv"));
    let end = rows(&first.join("entities-end.csv"));
    let births = rows(&first.join("births.csv"));
    let deaths = rows(&first.join("deaths.csv"));
    assert_eq!(start.len() + births.len() - deaths.len(), end.len(), "population balances over the window");
    let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(first.join("meta.json")).unwrap()).unwrap();
    assert_eq!((meta["width"].as_u64(), meta["height"].as_u64(), meta["start"].as_u64()), (Some(12), Some(12), Some(60)));
    let (success, text) = lab(&["run", s(&w), "--ticks", "10", "--threads", "1", "--out", s(&dir.join("c")), "--observe-window", "20", "--observe-count", "1"]);
    assert!(!success && text.contains("fit"), "{text}");
}

#[test]
fn counterfactual_forks_measure_what_members_achieve_without_partners() {
    let dir = scratch("forks");
    let w = world(&dir);
    ok(&["run", s(&w), "--ticks", "200", "--threads", "2", "--out", s(&dir.join("base"))]);
    let ckpt = dir.join("base/ckpt-200.bin");
    ok(&["dump", s(&ckpt), "--out", s(&dir.join("e.csv"))]);
    let entities = rows(&dir.join("e.csv"));
    // A B-maker (its second gene converts A -> B, resource 3).
    let maker = entities.iter().find(|e| e.last().unwrap().contains("convert:2>3")).unwrap()[0].clone();
    let sets = format!(r#"{{"sets": [{{"name": "all", "keep": null, "report": [{maker}]}}, {{"name": "solo", "keep": [{maker}], "report": [{maker}]}}]}}"#);
    std::fs::write(dir.join("sets.json"), sets).unwrap();
    let go = |out: &str, threads: &str| {
        ok(&["counterfactual", s(&ckpt), "--config", s(&w), "--sets", s(&dir.join("sets.json")), "--ticks", "300", "--threads", threads, "--out", s(&dir.join(out))]);
        std::fs::read_to_string(dir.join(out)).unwrap()
    };
    let text = go("r.json", "1");
    assert_eq!(text, go("r2.json", "4"), "forks are deterministic");
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let (all, solo) = (&json["results"][0], &json["results"][1]);
    assert!(all["final_population"].as_u64().unwrap() > 100);
    // Alone, a B-maker can only use C left over at the fork (in its own stocks and at its site),
    // so what its lineage makes of F stops growing; B, which it makes itself, keeps being made.
    let made = |r: &serde_json::Value, resource: usize| r["members"][0]["lineage_produced"][resource].as_u64().unwrap();
    assert!(solo["final_population"].as_u64().unwrap() <= 2);
    let sets = std::fs::read_to_string(dir.join("sets.json")).unwrap();
    assert!(sets.contains("solo"));
    ok(&["counterfactual", s(&ckpt), "--config", s(&w), "--sets", s(&dir.join("sets.json")), "--ticks", "600", "--threads", "1", "--out", s(&dir.join("long.json"))]);
    let long: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("long.json")).unwrap()).unwrap();
    let solo_long = &long["results"][1];
    if solo_long["members"][0]["alive"] == true {
        assert!(made(solo_long, 3) > made(solo, 3), "it keeps making its own product B");
        assert!(made(solo_long, 1) <= made(solo, 1) + 30, "but gets no new C to make F: {} then {}", made(solo, 1), made(solo_long, 1));
    }
    assert_eq!(all["members"][0]["id"], solo["members"][0]["id"]);
    std::fs::write(dir.join("bad.json"), r#"{"sets": [{"name": "x", "keep": [999999999], "report": []}]}"#).unwrap();
    let (success, message) = lab(&["counterfactual", s(&ckpt), "--config", s(&w), "--sets", s(&dir.join("bad.json")), "--ticks", "1", "--threads", "1", "--out", s(&dir.join("x.json"))]);
    assert!(!success && message.contains("not alive"), "{message}");
}
