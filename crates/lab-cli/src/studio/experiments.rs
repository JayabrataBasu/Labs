//! Experiments from the app: the same `python -m evolab` commands a person would type. Only
//! exploratory runs can be started here; registering an experiment stays a deliberate step.
use super::{named_toml, toml_names, valid_name, Folders};
use crate::edit::{error, header, html, json_response, Reply};
use serde_json::{json, Value};
use std::process::{Child, Command, Stdio};
use tiny_http::Response;

const LOG: &str = ".studio-experiment.log";
/// The page shows at most this much of the end of the log.
const LOG_TAIL: usize = 8000;

pub struct Runner {
    python_bin: String,
    running: Option<(String, Child)>,
    /// (spec, whether it succeeded) of the last finished experiment.
    last: Option<(String, bool)>,
}

impl Runner {
    pub fn new(python_bin: String) -> Self {
        Runner { python_bin, running: None, last: None }
    }

    pub fn poll(&mut self) {
        if let Some((name, child)) = self.running.as_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                self.last = Some((name.clone(), status.success()));
                self.running = None;
            }
        }
    }

    fn python(&self, folders: &Folders) -> Result<Command, Reply> {
        let lab = std::env::current_exe().map_err(|e| error(500, e))?;
        let mut command = Command::new(&self.python_bin);
        command.args(["-m", "evolab"]).env("PYTHONPATH", &folders.python).env("LAB_BIN", lab).stdin(Stdio::null());
        Ok(command)
    }

    pub fn list(&mut self, folders: &Folders) -> Reply {
        self.poll();
        let mut results: Vec<(String, usize, bool)> = std::fs::read_dir(&folders.results)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_str().filter(|n| valid_name(n))?.to_string();
                let summary = std::fs::read_to_string(entry.path().join("summary.csv")).ok()?;
                Some((name, summary.lines().count().saturating_sub(1), entry.path().join("report.html").is_file()))
            })
            .collect();
        results.sort();
        let results: Vec<Value> = results.into_iter().map(|(name, runs, report)| json!({ "name": name, "runs": runs, "report": report })).collect();
        json_response(
            200,
            json!({
                "specs": toml_names(&folders.experiments),
                "results": results,
                "running": self.running.as_ref().map(|(n, _)| n),
                "last": self.last.as_ref().map(|(n, ok)| json!({ "spec": n, "ok": ok })),
            }),
        )
    }

    pub fn log(&self, folders: &Folders) -> Reply {
        let bytes = std::fs::read(folders.results.join(LOG)).unwrap_or_default();
        let tail = String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(LOG_TAIL)..]).into_owned();
        Response::from_string(tail).with_header(header("Content-Type", "text/plain; charset=utf-8"))
    }

    pub fn start(&mut self, folders: &Folders, body: &Value) -> Result<Reply, Reply> {
        self.poll();
        if let Some((name, _)) = &self.running {
            return Err(error(409, format!("{name} is still running")));
        }
        let spec = named_toml(&folders.experiments, body["spec"].as_str())?;
        let jobs = body["jobs"].as_u64().filter(|&n| (1..=256).contains(&n)).ok_or_else(|| error(400, "jobs must be between 1 and 256"))?;
        std::fs::create_dir_all(&folders.results).map_err(|e| error(500, e))?;
        let log = std::fs::File::create(folders.results.join(LOG)).map_err(|e| error(500, e))?;
        let errors = log.try_clone().map_err(|e| error(500, e))?;
        let child = self
            .python(folders)?
            .arg("run")
            .arg(&spec)
            .arg("--out")
            .arg(&folders.results)
            .args(["--jobs", &jobs.to_string(), "--exploratory"])
            .stdout(log)
            .stderr(errors)
            .spawn()
            .map_err(|e| error(500, format!("cannot start {}: {e}", self.python_bin)))?;
        let name = body["spec"].as_str().unwrap_or_default().to_string();
        self.running = Some((name, child));
        Ok(json_response(200, json!({ "ok": true })))
    }

    pub fn report(&self, folders: &Folders, body: &Value) -> Result<Reply, Reply> {
        let name = body["name"].as_str().filter(|n| valid_name(n)).ok_or_else(|| error(400, "a result name is required"))?;
        let dir = folders.results.join(name);
        if !dir.join("summary.csv").is_file() {
            return Err(error(404, format!("{name} has no results")));
        }
        let done = self.python(folders)?.arg("html").arg(&dir).output().map_err(|e| error(500, e))?;
        if !done.status.success() {
            return Err(error(500, String::from_utf8_lossy(&done.stderr)));
        }
        Ok(json_response(200, json!({ "ok": true })))
    }
}

pub fn read_report(folders: &Folders, name: Option<&str>) -> Result<Reply, Reply> {
    let name = name.filter(|n| valid_name(n)).ok_or_else(|| error(400, "a result name is required"))?;
    let page = std::fs::read_to_string(folders.results.join(name).join("report.html")).map_err(|_| error(404, "no report yet"))?;
    Ok(html(page))
}
