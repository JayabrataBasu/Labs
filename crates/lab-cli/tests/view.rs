//! The viewer must be a pure observer: watching and pausing a run cannot change its result.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const REFERENCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../configs/reference.toml");

fn dir(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join("view").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}
/// Minimal HTTP/1.0 client (no chunked encoding): returns (status, body), or None if the server is not reachable.
fn http(port: u16, method: &str, path: &str, body: &str) -> Option<(u16, Vec<u8>)> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    write!(stream, "{method} {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{body}", body.len()).ok()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).ok()?;
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let status = std::str::from_utf8(&raw[..split]).ok()?.split_whitespace().nth(1)?.parse().ok()?;
    Some((status, raw[split + 4..].to_vec()))
}
fn meta(port: u16) -> serde_json::Value {
    let (status, body) = http(port, "GET", "/meta", "").expect("server reachable");
    assert_eq!(status, 200);
    serde_json::from_slice(&body).unwrap()
}
fn spawn_view(port: u16, ticks: &str, out: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["view", REFERENCE, "--ticks", ticks, "--threads", "2", "--port", &port.to_string(), "--out", out.to_str().unwrap()])
        .args(["--linger-seconds", "0", "--start-paused"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}
fn wait_until_up(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while http(port, "GET", "/meta", "").is_none() {
        assert!(Instant::now() < deadline, "viewer did not start");
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn final_hash(stdout: &[u8]) -> String {
    let text = String::from_utf8_lossy(stdout);
    let line = text.lines().find(|l| l.starts_with("final tick=")).unwrap_or_else(|| panic!("no final line in {text:?}"));
    line.split("hash=").nth(1).unwrap().trim().to_string()
}

#[test]
fn attached_equals_detached() {
    let (headless, viewed) = (dir("headless"), dir("viewed"));
    let run = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["run", REFERENCE, "--ticks", "200", "--threads", "2", "--out", headless.to_str().unwrap()])
        .output()
        .unwrap();
    let expected = final_hash(&run.stdout);

    let port = free_port();
    let mut child = spawn_view(port, "200", &viewed);
    wait_until_up(port);
    assert_eq!(meta(port)["tick"], 0, "--start-paused holds the run at tick 0");
    assert_eq!(meta(port)["paused"], true);

    // Single-step three ticks while paused.
    for expected_tick in 1..=3u64 {
        assert_eq!(http(port, "POST", "/control", "step").unwrap().0, 200);
        let deadline = Instant::now() + Duration::from_secs(10);
        while meta(port)["tick"] != expected_tick {
            assert!(Instant::now() < deadline, "step did not advance to tick {expected_tick}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(meta(port)["tick"], 3, "a paused run does not advance on its own");

    // Frame layout while paused: tick, then per-site stock, then per-site entity count.
    let (status, frame) = http(port, "GET", "/frame?resource=0", "").unwrap();
    assert_eq!(status, 200);
    assert_eq!(frame.len(), 8 + 8 * 4096);
    assert_eq!(u64::from_le_bytes(frame[..8].try_into().unwrap()), 3);
    let counts: u64 = frame[8 + 4 * 4096..].as_chunks::<4>().0.iter().map(|c| u64::from(u32::from_le_bytes(*c))).sum();
    assert_eq!(counts, 2000);
    assert_eq!(http(port, "GET", "/frame?resource=9", "").unwrap().0, 400);
    assert_eq!(http(port, "POST", "/control", "explode").unwrap().0, 400);
    assert_eq!(http(port, "GET", "/nope", "").unwrap().0, 404);
    let (status, page) = http(port, "GET", "/", "").unwrap();
    assert_eq!(status, 200);
    let page = String::from_utf8(page).unwrap();
    assert!(page.contains("<canvas"));
    assert!(!page.contains("http://") && !page.contains("https://"), "the viewer page must not load anything external");

    // Resume, then hammer the server until the run finishes.
    assert_eq!(http(port, "POST", "/control", "pause").unwrap().0, 200);
    assert_eq!(http(port, "POST", "/control", "resume").unwrap().0, 200);
    while child.try_wait().unwrap().is_none() {
        let _ = http(port, "GET", "/frame?resource=1", "");
        let _ = http(port, "GET", "/meta", "");
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(final_hash(&output.stdout), expected, "watching, pausing and stepping must not change the result");
    assert!(viewed.join("manifest.json").exists());
    assert!(viewed.join("ckpt-200.bin").exists());
}

#[test]
fn port_in_use_is_clean_error() {
    let holder = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = holder.local_addr().unwrap().port();
    let out = spawn_view(port, "5", &dir("port-busy")).wait_with_output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(&format!("port {port}")), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}
