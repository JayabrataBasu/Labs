//! `lab studio`: one local app to edit worlds, watch runs and run experiments.
//! Watching, pausing, stepping and inspecting must never change a result.
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

struct Studio {
    child: Child,
    port: u16,
    dir: PathBuf,
}
impl Drop for Studio {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn python() -> &'static str {
    if cfg!(windows) { "python" } else { "python3" }
}

fn start(name: &str) -> Studio {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("studio").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["worlds", "results", "experiments"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    std::fs::copy(Path::new(ROOT).join("configs/reference.toml"), dir.join("worlds/ref.toml")).unwrap();
    // A small, busy containment world, so that guests appear within a few ticks.
    let small = std::fs::read_to_string(Path::new(ROOT).join("configs/containment.toml"))
        .unwrap()
        .replace("width = 48", "width = 12")
        .replace("height = 48", "height = 12")
        .replace("count = 800", "count = 300")
        .replace("probability_ppm = 20000, max_guests = 3", "probability_ppm = 800000, max_guests = 3");
    std::fs::write(dir.join("worlds/cont.toml"), small).unwrap();
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let arg = |sub: &str| dir.join(sub).to_str().unwrap().to_string();
    let child = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["studio", "--port", &port.to_string(), "--worlds", &arg("worlds"), "--results", &arg("results")])
        .args(["--experiments", &arg("experiments"), "--python", &format!("{ROOT}/python"), "--python-bin", python()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let studio = Studio { child, port, dir };
    let deadline = Instant::now() + Duration::from_secs(20);
    while studio.request("GET", "/worlds", None, "").is_none() {
        assert!(Instant::now() < deadline, "studio did not start");
        std::thread::sleep(Duration::from_millis(20));
    }
    studio
}

impl Studio {
    fn request(&self, method: &str, path: &str, host: Option<&str>, body: &str) -> Option<(u16, Vec<u8>)> {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).ok()?;
        stream.set_read_timeout(Some(Duration::from_secs(60))).ok()?;
        let host = host.map_or(format!("127.0.0.1:{}", self.port), str::to_string);
        let kind = if body.starts_with('{') { "Content-Type: application/json\r\n" } else { "" };
        write!(stream, "{method} {path} HTTP/1.0\r\nHost: {host}\r\n{kind}Content-Length: {}\r\n\r\n{body}", body.len()).ok()?;
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).ok()?;
        let split = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
        let status = std::str::from_utf8(&raw[..split]).ok()?.split_whitespace().nth(1)?.parse().ok()?;
        Some((status, raw[split + 4..].to_vec()))
    }
    fn get(&self, path: &str) -> (u16, Vec<u8>) {
        self.request("GET", path, None, "").expect("studio reachable")
    }
    fn json(&self, path: &str) -> Value {
        let (status, body) = self.get(path);
        assert_eq!(status, 200, "{path}: {}", String::from_utf8_lossy(&body));
        serde_json::from_slice(&body).unwrap()
    }
    fn post(&self, path: &str, body: Value) -> (u16, String) {
        let (status, text) = self.request("POST", path, None, &body.to_string()).expect("studio reachable");
        (status, String::from_utf8_lossy(&text).into_owned())
    }
    fn ok(&self, path: &str, body: Value) -> Value {
        let (status, text) = self.post(path, body);
        assert_eq!(status, 200, "{path}: {text}");
        serde_json::from_str(&text).unwrap()
    }
    fn wait(&self, what: &str, done: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let status = self.json("/status");
            if done(&status) {
                return status;
            }
            assert!(Instant::now() < deadline, "waiting for {what}: {status}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn layer(&self, query: &str) -> (u64, Vec<u32>) {
        let (status, bytes) = self.get(&format!("/layer?{query}"));
        assert_eq!(status, 200, "{query}: {}", String::from_utf8_lossy(&bytes));
        let values = bytes[8..].as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect();
        (u64::from_le_bytes(bytes[..8].try_into().unwrap()), values)
    }
}

fn headless(config: &Path, ticks: u64, out: &Path) -> String {
    let run = Command::new(env!("CARGO_BIN_EXE_lab"))
        .args(["run", config.to_str().unwrap(), "--ticks", &ticks.to_string(), "--threads", "1", "--out", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    String::from_utf8_lossy(&run.stdout).split("hash=").nth(1).unwrap().trim().to_string()
}
fn manifest(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap()
}

#[test]
fn pages_and_worlds_are_served_to_this_machine_only() {
    let s = start("pages");
    for path in ["/", "/editor?world=ref"] {
        let (status, page) = s.get(path);
        let page = String::from_utf8(page).unwrap();
        assert_eq!(status, 200, "{path}");
        assert!(page.contains("<canvas"), "{path}");
        assert!(!page.contains("http://") && !page.contains("https://"), "{path} must be self-contained");
    }
    assert_eq!(s.json("/worlds"), json!(["cont", "ref"]));
    assert!(s.json("/config?world=ref")["seed"].is_string());
    assert!(s.json("/catalog").as_array().unwrap().len() >= 18);
    assert_eq!(s.request("GET", "/worlds", Some("evil.example"), "").unwrap().0, 403);
    for bad in ["/config?world=../worlds/ref", "/config?world=nope", "/config", "/config?world=.hidden"] {
        assert!(matches!(s.get(bad).0, 400 | 404), "{bad}: {}", s.get(bad).0);
    }
    assert_eq!(s.get("/nothing").0, 404);
}

#[test]
fn worlds_are_copied_and_saved_one_at_a_time_inside_the_worlds_folder() {
    let s = start("save");
    let original = std::fs::read_to_string(s.dir.join("worlds/ref.toml")).unwrap();
    s.ok("/worlds/copy", json!({"from": "ref", "to": "mine"}));
    assert_eq!(s.json("/worlds"), json!(["cont", "mine", "ref"]));
    assert_eq!(s.post("/worlds/copy", json!({"from": "ref", "to": "mine"})).0, 400, "never overwrites");
    for bad in ["../escape", "a/b", ".x", ""] {
        assert_eq!(s.post("/worlds/copy", json!({"from": "ref", "to": bad})).0, 400, "{bad:?}");
    }
    assert!(!s.dir.join("escape.toml").exists());
    let mut cfg = s.json("/config?world=mine");
    cfg["seed"] = "77".into();
    s.ok("/save?world=mine", cfg.clone());
    assert_eq!(s.json("/config?world=mine")["seed"], "77");
    assert_eq!(std::fs::read_to_string(s.dir.join("worlds/ref.toml")).unwrap(), original);
    cfg["world"]["width"] = 0.into();
    assert_eq!(s.post("/save?world=mine", cfg).0, 400);
    assert_eq!(s.request("POST", "/save?world=mine", None, "not json").unwrap().0, 415);
    let huge = format!("{{\"x\": \"{}\"}}", "a".repeat(2 << 20));
    assert_eq!(s.request("POST", "/save?world=mine", None, &huge).map_or(413, |r| r.0), 413);
}

#[test]
fn a_watched_run_gives_the_same_result_as_a_headless_one() {
    let s = start("watch");
    let expected = headless(&s.dir.join("worlds/ref.toml"), 200, &s.dir.join("headless"));
    assert_eq!(s.json("/status")["active"], false);
    assert_eq!(s.get("/layer?kind=count").0, 404, "no run yet");
    let run = s.ok("/run", json!({"world": "ref", "ticks": 200, "threads": 2, "paused": true}))["run"].as_str().unwrap().to_string();
    let status = s.wait("start", |v| v["active"] == true && v["tick"] == 0);
    assert_eq!((status["paused"].clone(), status["width"].clone(), status["entities"].clone()), (json!(true), json!(64), json!(2000)));
    for tick in 1..=3 {
        s.ok("/control", json!({"command": "step"}));
        s.wait("step", |v| v["tick"] == tick);
    }
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(s.json("/status")["tick"], 3, "a paused run does not advance");
    let (tick, counts) = s.layer("kind=count");
    assert_eq!((tick, counts.len(), counts.iter().sum::<u32>()), (3, 4096, 2000));
    assert_eq!(s.layer("kind=stock&resource=0").1.len(), 4096);
    for bad in ["kind=stock&resource=9", "kind=stock", "kind=bogus"] {
        assert_eq!(s.get(&format!("/layer?{bad}")).0, 400, "{bad}");
    }
    assert_eq!(s.post("/control", json!({"command": "explode"})).0, 400);
    s.ok("/control", json!({"speed": 2000}));
    s.ok("/control", json!({"command": "resume"}));
    let done = loop {
        let _ = (s.layer("kind=type"), s.json("/history"), s.json("/types"), s.json("/site?x=0&y=0"));
        let status = s.json("/status");
        if status["done"] == true {
            break status;
        }
    };
    assert_eq!(done["final_hash"], expected.as_str());
    assert_eq!(done["stopped"], false);
    let history = s.json("/history");
    let ticks = history["tick"].as_array().unwrap();
    assert_eq!(ticks.last().unwrap(), 200);
    assert_eq!(history["entities"].as_array().unwrap().len(), ticks.len());
    assert_eq!(history["site"].as_array().unwrap().len(), done["resources"].as_array().unwrap().len());
    let dir = s.dir.join("results/studio").join(run);
    assert_eq!(manifest(&dir)["final_sha256"], expected.as_str());
    assert!(dir.join("config.toml").exists() && dir.join("series.csv").exists() && dir.join("ckpt-200.bin").exists());
}

#[test]
fn entities_hosts_and_types_can_be_inspected() {
    let s = start("inspect");
    s.ok("/run", json!({"world": "cont", "ticks": 1000, "threads": 1, "paused": true}));
    s.wait("start", |v| v["active"] == true);
    for tick in 1..=15 {
        s.ok("/control", json!({"command": "step"}));
        s.wait("step", |v| v["tick"] == tick);
    }
    let status = s.json("/status");
    let contained = status["contained"].as_u64().unwrap();
    assert!(contained > 0, "{status}");
    assert_eq!(u64::from(s.layer("kind=guests").1.iter().sum::<u32>()), contained);
    let types = s.json("/types");
    assert!(types.as_array().unwrap().iter().any(|t| t["label"].as_str().unwrap().contains("A→B")), "{types}");
    let codes: Vec<u64> = types.as_array().unwrap().iter().map(|t| t["code"].as_u64().unwrap()).collect();
    assert!(s.layer("kind=type").1.iter().all(|&c| c == 0 || codes.contains(&u64::from(c))));

    let mut found = false;
    for site in 0..144 {
        let here = s.json(&format!("/site?x={}&y={}", site % 12, site / 12));
        for e in here["entities"].as_array().unwrap() {
            let Some(guest) = e["guests"].as_array().unwrap().first().and_then(Value::as_u64) else { continue };
            let host = e["id"].as_u64().unwrap();
            let g = s.json(&format!("/entity?id={guest}"));
            assert_eq!((g["host"].as_u64(), g["site"].as_u64()), (Some(host), Some(site)));
            assert!(g["genes"].as_array().unwrap().iter().any(|x| x.as_str().unwrap().starts_with("bind ")), "{g}");
            assert!(g["stocks"]["E"].is_u64() && g["type"].is_string());
            found = true;
        }
    }
    assert!(found);
    assert_eq!(s.get("/entity?id=999999999").0, 404);
    assert_eq!(s.get("/site?x=12&y=0").0, 400);
}

#[test]
fn a_stopped_run_is_complete_and_a_new_run_replaces_the_old_one() {
    let s = start("stop");
    let first = s.ok("/run", json!({"world": "cont", "ticks": 1000000, "threads": 1, "paused": false}))["run"].as_str().unwrap().to_string();
    s.wait("progress", |v| v["tick"].as_u64().unwrap_or(0) > 5);
    s.ok("/control", json!({"command": "stop"}));
    let done = s.wait("stop", |v| v["done"] == true);
    assert_eq!(done["stopped"], true);
    let tick = done["tick"].as_u64().unwrap();
    let dir = s.dir.join("results/studio").join(&first);
    assert_eq!(manifest(&dir)["end_tick"], tick);
    let hash = Command::new(env!("CARGO_BIN_EXE_lab")).args(["hash", dir.join(format!("ckpt-{tick}.bin")).to_str().unwrap()]).output().unwrap();
    assert!(String::from_utf8_lossy(&hash.stdout).contains(done["final_hash"].as_str().unwrap()));

    let second = s.ok("/run", json!({"world": "cont", "ticks": 1000000, "threads": 1, "paused": true}))["run"].as_str().unwrap().to_string();
    let third = s.ok("/run", json!({"world": "ref", "ticks": 30, "threads": 1, "paused": false, "seed": "12345"}))["run"].as_str().unwrap().to_string();
    assert!(first != second && second != third);
    assert!(s.dir.join("results/studio").join(&second).join("manifest.json").exists(), "the replaced run was finished off properly");
    let done = s.wait("third", |v| v["done"] == true);
    let config = s.dir.join("results/studio").join(&third).join("config.toml");
    assert!(std::fs::read_to_string(&config).unwrap().contains("seed = 12345"));
    assert_eq!(done["final_hash"], headless(&config, 30, &s.dir.join("again")).as_str());
    assert_eq!(s.post("/run", json!({"world": "nope", "ticks": 5, "threads": 1, "paused": false})).0, 400);
    assert_eq!(s.post("/run", json!({"world": "ref", "ticks": 5, "threads": 0, "paused": false})).0, 400);
}

#[test]
fn experiments_run_from_the_app_and_produce_a_report() {
    if Command::new(python()).arg("--version").output().is_err() {
        eprintln!("skipped: no {} on this machine", python());
        return;
    }
    let s = start("experiments");
    std::fs::copy(s.dir.join("worlds/cont.toml"), s.dir.join("experiments/base.toml")).unwrap();
    let spec = "name = \"tiny\"\nbase_config = \"base.toml\"\nticks = 40\nseries_every = 10\nthreads = 1\nseeds = [1, 2]\n\
                primary_outcome = \"entities\"\naim = \"test\"\nestimand = \"test\"\nanalysis_plan = \"test\"\n\
                [[arms]]\nname = \"intact\"\ntransforms = []\n[[arms]]\nname = \"alone\"\ntransforms = [{ name = \"null.no_sharing\" }]\n";
    std::fs::write(s.dir.join("experiments/tiny.toml"), spec).unwrap();
    let list = s.json("/experiments");
    assert!(list["specs"].as_array().unwrap().contains(&json!("tiny")), "{list}");
    assert_eq!(s.post("/experiments/run", json!({"spec": "../tiny", "jobs": 1})).0, 400);
    s.ok("/experiments/run", json!({"spec": "tiny", "jobs": 2}));
    let deadline = Instant::now() + Duration::from_secs(120);
    let list = loop {
        let list = s.json("/experiments");
        if list["running"].is_null() {
            break list;
        }
        assert!(Instant::now() < deadline, "{list}");
        std::thread::sleep(Duration::from_millis(100));
    };
    let log = String::from_utf8_lossy(&s.get("/experiments/log").1).into_owned();
    assert_eq!(list["results"], json!([{"name": "tiny", "runs": 4, "report": false}]), "log: {log}");
    assert_eq!(s.get("/report?name=tiny").0, 404);
    s.ok("/experiments/report", json!({"name": "tiny"}));
    let (status, page) = s.get("/report?name=tiny");
    let page = String::from_utf8(page).unwrap();
    assert_eq!(status, 200);
    assert!(page.contains("tiny") && page.contains("<svg"));
}
