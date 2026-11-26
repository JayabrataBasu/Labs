//! The world editor's server: it must round-trip configs, validate saves, and refuse foreign requests.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const CROSSFEED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../configs/crossfeed.toml");

fn scratch(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join("edit").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}
fn request(port: u16, method: &str, path: &str, content_type: Option<&str>, host: &str, body: &str) -> Option<(u16, String)> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let ct = content_type.map(|c| format!("Content-Type: {c}\r\n")).unwrap_or_default();
    write!(stream, "{method} {path} HTTP/1.0\r\nHost: {host}\r\n{ct}Content-Length: {}\r\n\r\n{body}", body.len()).ok()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).ok()?;
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n")?;
    Some((head.split_whitespace().nth(1)?.parse().ok()?, body.to_string()))
}
fn get(port: u16, path: &str) -> (u16, String) {
    request(port, "GET", path, None, &format!("127.0.0.1:{port}"), "").expect("server reachable")
}
fn save(port: u16, body: &str) -> (u16, String) {
    request(port, "POST", "/save", Some("application/json"), &format!("127.0.0.1:{port}"), body).expect("server reachable")
}
struct Editor(Child, u16);
impl Drop for Editor {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn start(config: &Path) -> Editor {
    let port = free_port();
    let child = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["edit", config.to_str().unwrap(), "--port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while request(port, "GET", "/catalog", None, &format!("127.0.0.1:{port}"), "").is_none() {
        assert!(Instant::now() < deadline, "editor did not start");
        std::thread::sleep(Duration::from_millis(20));
    }
    Editor(child, port)
}

#[test]
fn page_and_catalog_are_served() {
    let dir = scratch("page");
    let file = dir.join("w.toml");
    std::fs::copy(CROSSFEED, &file).unwrap();
    let ed = start(&file);
    let (status, page) = get(ed.1, "/");
    assert_eq!(status, 200);
    assert!(page.contains("<canvas") && !page.contains("http://") && !page.contains("https://"), "the page must be self-contained");
    let (status, body) = get(ed.1, "/catalog");
    assert_eq!(status, 200);
    let catalog: serde_json::Value = serde_json::from_str(&body).unwrap();
    let ids: Vec<&str> = catalog.as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), 14);
    for id in ["metabolism.genes", "shock.scheduled", "reproduction.division", "mix.relocate"] {
        assert!(ids.contains(&id), "{id} missing from the catalog");
    }
    assert_eq!(get(ed.1, "/nope").0, 404);
}

#[test]
fn config_roundtrips_through_save_unchanged() {
    let dir = scratch("roundtrip");
    let file = dir.join("w.toml");
    std::fs::copy(CROSSFEED, &file).unwrap();
    let ed = start(&file);
    let (status, body) = get(ed.1, "/config");
    assert_eq!(status, 200);
    let before: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(before["seed"].is_string(), "the seed travels as a string");
    let (status, result) = save(ed.1, &body);
    assert_eq!(status, 200, "{result}");
    assert!(dir.join("w.toml.bak").exists(), "the previous file is kept");
    assert_eq!(std::fs::read_to_string(dir.join("w.toml.bak")).unwrap(), std::fs::read_to_string(CROSSFEED).unwrap());
    let after: serde_json::Value = serde_json::from_str(&get(ed.1, "/config").1).unwrap();
    assert_eq!(before, after, "saving what the editor loaded must not change the world");

    // The saved file still runs, and its run matches the original world's trajectory.
    let run = |config: &Path, out: &Path| {
        let output = Command::new(env!("CARGO_BIN_EXE_lab"))
            .args(["run", config.to_str().unwrap(), "--ticks", "60", "--threads", "2", "--out", out.to_str().unwrap(), "--series-every", "60"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        std::fs::read_to_string(out.join("series.csv")).unwrap()
    };
    assert_eq!(run(&file, &dir.join("a")), run(Path::new(CROSSFEED), &dir.join("b")), "same world, same series (hashes differ only by config text)");
}

#[test]
fn invalid_saves_are_refused_and_change_nothing() {
    let dir = scratch("invalid");
    let file = dir.join("w.toml");
    std::fs::copy(CROSSFEED, &file).unwrap();
    let original = std::fs::read_to_string(&file).unwrap();
    let ed = start(&file);
    let good: serde_json::Value = serde_json::from_str(&get(ed.1, "/config").1).unwrap();

    let mut bad_rule = good.clone();
    bad_rule["rules"][0]["params"]["resource"] = "Z".into();
    let mut bad_seed = good.clone();
    bad_seed["seed"] = "not a number".into();
    let mut unknown_field = good.clone();
    unknown_field["bogus"] = 1.into();
    let mut bad_patch = good.clone();
    bad_patch["init"]["patches"] = serde_json::json!([{ "x": 40, "y": 0, "width": 20, "height": 1, "resource": "A", "amount": 1 }]);
    for (name, body) in [("unknown resource", bad_rule), ("seed", bad_seed), ("unknown field", unknown_field), ("patch outside", bad_patch)] {
        let (status, text) = save(ed.1, &body.to_string());
        assert_eq!(status, 400, "{name}: {text}");
        assert!(text.contains("error"), "{name}: {text}");
    }
    assert_eq!(save(ed.1, "{ not json").0, 400);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original, "a refused save must not touch the file");
    assert!(!dir.join("w.toml.bak").exists());
}

#[test]
fn edits_made_in_the_page_reach_the_file() {
    let dir = scratch("edits");
    let file = dir.join("w.toml");
    std::fs::copy(CROSSFEED, &file).unwrap();
    let ed = start(&file);
    let mut cfg: serde_json::Value = serde_json::from_str(&get(ed.1, "/config").1).unwrap();
    cfg["world"]["width"] = 20.into();
    cfg["world"]["height"] = 12.into();
    cfg["seed"] = "18446744073709551615".into();
    cfg["init"]["founders"][0]["count"] = 7.into();
    cfg["init"]["patches"] = serde_json::json!([{ "x": 2, "y": 3, "width": 4, "height": 5, "resource": "A", "amount": 77 }]);
    cfg["rules"].as_array_mut().unwrap().push(serde_json::json!({
        "id": "shock.scheduled", "version": 1, "stream": 99,
        "params": { "tick": 10, "action": "add", "resource": "A", "amount": 5, "x": 0, "y": 0, "width": 3, "height": 3 }
    }));
    let (status, text) = save(ed.1, &cfg.to_string());
    assert_eq!(status, 400, "a seed above the TOML integer range is refused cleanly: {text}");
    cfg["seed"] = "9223372036854775807".into();
    let (status, text) = save(ed.1, &cfg.to_string());
    assert_eq!(status, 200, "{text}");
    let saved = std::fs::read_to_string(&file).unwrap();
    assert!(saved.contains("seed = 9223372036854775807") && saved.contains("amount = 77") && saved.contains("shock.scheduled"), "{saved}");
    let again: serde_json::Value = serde_json::from_str(&get(ed.1, "/config").1).unwrap();
    assert_eq!(again["world"]["width"], 20);
    assert_eq!(again["seed"], "9223372036854775807", "no rounding of large seeds");
}

#[test]
fn foreign_requests_are_refused() {
    let dir = scratch("foreign");
    let file = dir.join("w.toml");
    std::fs::copy(CROSSFEED, &file).unwrap();
    let original = std::fs::read_to_string(&file).unwrap();
    let ed = start(&file);
    let body = get(ed.1, "/config").1;
    let (status, _) = request(ed.1, "GET", "/config", None, "evil.example:80", "").unwrap();
    assert_eq!(status, 403, "a request addressed to another host (DNS rebinding) is refused");
    let host = format!("127.0.0.1:{}", ed.1);
    for content_type in [Some("text/plain"), Some("application/x-www-form-urlencoded"), None] {
        let (status, _) = request(ed.1, "POST", "/save", content_type, &host, &body).unwrap();
        assert_eq!(status, 415, "{content_type:?}");
    }
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
}

#[test]
fn port_in_use_is_clean_error() {
    let holder = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = holder.local_addr().unwrap().port();
    let dir = scratch("busy");
    let file = dir.join("w.toml");
    std::fs::copy(CROSSFEED, &file).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lab")).args(["edit", file.to_str().unwrap(), "--port", &port.to_string()]).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(&format!("port {port}")) && !stderr.contains("panicked"), "{stderr}");
}
