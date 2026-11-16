//! World editor: a local web page that edits one config file visually and saves it.
//!
//! The server only ever reads and writes the single config path it was started with. A save is
//! validated by the same parser a run uses, so the editor cannot write a config that fails to
//! load. The page contains no science: it edits the declarative config and nothing else.
use lab_core::config::{parse, Config};
use lab_core::LabError;
use std::path::Path;
use std::time::Duration;
use tiny_http::{Header, Method, Request, Response, Server};

const PAGE: &str = include_str!("editor.html");
const BIND_ADDRESS: &str = "127.0.0.1";

/// A config as the page sees it. The seed travels as a string so JavaScript cannot round it.
pub fn config_to_json(cfg: &Config) -> Result<serde_json::Value, LabError> {
    let mut value = serde_json::to_value(cfg).map_err(|e| LabError::Config(e.to_string()))?;
    value["seed"] = serde_json::Value::String(cfg.seed.to_string());
    Ok(value)
}

/// Validates a page-supplied config and renders it as canonical TOML.
pub fn json_to_toml(mut value: serde_json::Value) -> Result<String, LabError> {
    if let Some(seed) = value.get("seed").and_then(|s| s.as_str()).map(str::to_owned) {
        let seed: u64 = seed.trim().parse().map_err(|_| LabError::Config("seed must be a whole number".into()))?;
        value["seed"] = serde_json::Value::from(seed);
    }
    let cfg: Config = serde_json::from_value(value).map_err(|e| LabError::Config(e.to_string()))?;
    let text = toml::to_string(&cfg).map_err(|e| LabError::Config(format!("cannot write config: {e}")))?;
    // Parsing checks the world; building the rules checks every rule's parameters, exactly as a run would.
    let (checked, _) = parse(&text)?;
    lab_rules::build(&checked)?;
    Ok(text)
}

pub fn edit(config_path: &Path, port: u16) -> Result<(), LabError> {
    let server = Server::http((BIND_ADDRESS, port)).map_err(|e| LabError::Io(format!("cannot listen on port {port}: {e}")))?;
    eprintln!("editor: http://{BIND_ADDRESS}:{port}/  (editing {})", config_path.display());
    eprintln!("press Ctrl-C to stop");
    loop {
        if let Ok(Some(request)) = server.recv_timeout(Duration::from_millis(200)) {
            handle(request, config_path, port);
        }
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header is valid")
}

fn json_response(status: u16, value: serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(value.to_string()).with_status_code(status).with_header(header("Content-Type", "application/json"))
}

fn host_ok(request: &Request, port: u16) -> bool {
    request.headers().iter().find(|h| h.field.equiv("Host")).is_some_and(|h| {
        let host = h.value.as_str();
        host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
    })
}

pub fn handle(mut request: Request, config_path: &Path, port: u16) {
    // Refuse requests whose Host header is not this local server (guards against DNS rebinding).
    if !host_ok(&request, port) {
        let _ = request.respond(Response::from_string("bad host").with_status_code(403));
        return;
    }
    let url = request.url().to_string();
    let response = match (request.method(), url.as_str()) {
        (Method::Get, "/") => Response::from_string(PAGE).with_header(header("Content-Type", "text/html; charset=utf-8")),
        (Method::Get, "/config") => match std::fs::read_to_string(config_path)
            .map_err(|e| LabError::Io(format!("cannot read {}: {e}", config_path.display())))
            .and_then(|text| parse(&text).map(|(cfg, _)| cfg))
            .and_then(|cfg| config_to_json(&cfg))
        {
            Ok(value) => json_response(200, value),
            Err(e) => json_response(400, serde_json::json!({ "error": e.to_string() })),
        },
        (Method::Get, "/catalog") => {
            let rules: Vec<_> = lab_rules::CATALOG
                .iter()
                .map(|(id, about, params)| serde_json::json!({ "id": id, "about": about, "params": serde_json::from_str::<serde_json::Value>(params).expect("catalog JSON is valid") }))
                .collect();
            json_response(200, serde_json::Value::Array(rules))
        }
        (Method::Post, "/save") => {
            // A cross-origin page cannot send application/json without a preflight, which this server never grants.
            let is_json = request.headers().iter().any(|h| h.field.equiv("Content-Type") && h.value.as_str().starts_with("application/json"));
            if !is_json {
                json_response(415, serde_json::json!({ "error": "expected application/json" }))
            } else {
                let mut body = String::new();
                let _ = request.as_reader().read_to_string(&mut body);
                match serde_json::from_str::<serde_json::Value>(&body).map_err(|e| LabError::Config(e.to_string())).and_then(json_to_toml) {
                    Ok(text) => save(config_path, &text),
                    Err(e) => json_response(400, serde_json::json!({ "error": e.to_string() })),
                }
            }
        }
        _ => Response::from_string("not found").with_status_code(404),
    };
    let _ = request.respond(response.with_header(header("Cache-Control", "no-store")));
}

fn save(config_path: &Path, text: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let backup = config_path.with_extension("toml.bak");
    if config_path.exists() {
        if let Err(e) = std::fs::copy(config_path, &backup) {
            return json_response(500, serde_json::json!({ "error": format!("cannot back up the old file: {e}") }));
        }
    }
    match std::fs::write(config_path, text) {
        Ok(()) => json_response(200, serde_json::json!({ "saved": config_path.display().to_string(), "backup": backup.display().to_string(), "bytes": text.len() })),
        Err(e) => json_response(500, serde_json::json!({ "error": format!("cannot write {}: {e}", config_path.display()) })),
    }
}
