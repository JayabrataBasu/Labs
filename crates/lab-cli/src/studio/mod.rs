//! `lab studio`: one local web app joining the world editor, live runs and experiments.
//!
//! The server listens on 127.0.0.1 only, checks the Host header, accepts only JSON bodies and
//! resolves every name inside the folders it was started with. It holds no science: it edits
//! declarative configs, runs them with the same code as `lab run`, and shows what is there.
mod experiments;
mod inspect;
mod session;

use crate::edit::{self, error, header, html, json_response, Reply};
use lab_core::LabError;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tiny_http::{Method, Request, Response, Server};

const PAGE: &str = include_str!("../studio.html");
const BIND_ADDRESS: &str = "127.0.0.1";

pub struct Folders {
    pub worlds: PathBuf,
    pub results: PathBuf,
    pub experiments: PathBuf,
    pub python: PathBuf,
}

struct Studio {
    folders: Folders,
    live: Option<session::LiveRun>,
    experiments: experiments::Runner,
}

pub fn serve(port: u16, folders: Folders, python_bin: String) -> Result<(), LabError> {
    if !folders.worlds.is_dir() {
        return Err(LabError::Config(format!("the worlds folder {} does not exist (see --worlds)", folders.worlds.display())));
    }
    let server = Server::http((BIND_ADDRESS, port)).map_err(|e| LabError::Io(format!("cannot listen on port {port}: {e}")))?;
    eprintln!("studio: http://{BIND_ADDRESS}:{port}/");
    eprintln!(
        "worlds {}  results {}  experiments {}",
        folders.worlds.display(),
        folders.results.display(),
        folders.experiments.display()
    );
    eprintln!("press Ctrl-C to stop");
    let mut studio = Studio { folders, live: None, experiments: experiments::Runner::new(python_bin) };
    loop {
        if let Ok(Some(mut request)) = server.recv_timeout(Duration::from_millis(100)) {
            let reply = if edit::host_ok(&request, port) { route(&mut studio, &mut request) } else { error(403, "bad host") };
            let _ = request.respond(reply.with_header(header("Cache-Control", "no-store")));
        }
        studio.experiments.poll();
    }
}

/// A file name given by the page: no separators, no leading dot, so it stays inside its folder.
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && !name.starts_with('.') && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

fn param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| pair.split_once('=').filter(|(k, _)| *k == key).map(|(_, v)| v))
}

/// Names of the `.toml` files in a folder, without the extension.
fn toml_names(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_string))
        .filter(|n| valid_name(n))
        .collect();
    names.sort();
    names
}

/// The path of an existing `<folder>/<name>.toml`.
fn named_toml(folder: &Path, name: Option<&str>) -> Result<PathBuf, Reply> {
    let name = name.filter(|n| valid_name(n)).ok_or_else(|| error(400, "a name made of letters, digits, '-', '_' and '.' is required"))?;
    let path = folder.join(format!("{name}.toml"));
    if path.is_file() {
        Ok(path)
    } else {
        Err(error(400, format!("there is no {}", path.display())))
    }
}

fn route(studio: &mut Studio, request: &mut Request) -> Reply {
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let post = *request.method() == Method::Post;
    if !post && *request.method() != Method::Get {
        return error(405, "only GET and POST");
    }
    let body = match (post, path) {
        // Saving a world reads and checks its own body.
        (true, "/save") | (false, _) => Value::Null,
        (true, _) => match edit::read_json(request) {
            Ok(value) => value,
            Err(reply) => return reply,
        },
    };
    let reply = match (post, path) {
        (false, "/") => Ok(html(PAGE)),
        (false, "/editor") => Ok(html(edit::PAGE)),
        (false, "/catalog") => Ok(edit::catalog_response()),
        (false, "/worlds") => Ok(json_response(200, json!(toml_names(&studio.folders.worlds)))),
        (false, "/config") => named_toml(&studio.folders.worlds, param(query, "world")).map(|p| edit::config_response(&p)),
        (true, "/save") => named_toml(&studio.folders.worlds, param(query, "world")).map(|p| edit::save_response(request, &p)),
        (true, "/worlds/copy") => copy_world(&studio.folders.worlds, &body),
        (true, "/run") => session::start(studio, &body),
        (true, "/control") => session::control(studio.live.as_ref(), &body),
        (false, "/status") => Ok(json_response(200, session::status(studio.live.as_ref()))),
        (false, "/history" | "/layer" | "/types" | "/site" | "/entity") => match studio.live.as_ref() {
            Some(live) => inspect::get(live, path, query),
            None => Err(error(404, "no run has been started")),
        },
        (false, "/experiments") => Ok(studio.experiments.list(&studio.folders)),
        (false, "/experiments/log") => Ok(studio.experiments.log(&studio.folders)),
        (true, "/experiments/run") => studio.experiments.start(&studio.folders, &body),
        (true, "/experiments/report") => studio.experiments.report(&studio.folders, &body),
        (false, "/report") => experiments::read_report(&studio.folders, param(query, "name")),
        _ => Err(Response::from_string("not found").with_status_code(404)),
    };
    reply.unwrap_or_else(|e| e)
}

fn copy_world(worlds: &Path, body: &Value) -> Result<Reply, Reply> {
    let from = named_toml(worlds, body["from"].as_str())?;
    let to = body["to"].as_str().filter(|n| valid_name(n)).ok_or_else(|| error(400, "the new name may use letters, digits, '-', '_' and '.'"))?;
    let target = worlds.join(format!("{to}.toml"));
    if target.exists() {
        return Err(error(400, format!("{to} already exists")));
    }
    std::fs::copy(&from, &target).map_err(|e| error(500, e))?;
    Ok(json_response(200, json!({ "created": to })))
}
