//! Read-only live viewer. The simulation publishes a copy of what the page needs after each
//! tick; HTTP handlers only ever read that copy, so observing a run cannot change it.
use crate::run::{execute, hex, Session};
use lab_core::{LabError, State};
use serde_json::json;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use tiny_http::{Header, Method, Request, Response, Server};

const PAGE: &str = include_str!("viewer.html");
/// The viewer is a local tool: it never listens on a network-facing address.
const BIND_ADDRESS: &str = "127.0.0.1";

pub struct ViewOptions<'a> {
    pub ticks: u64,
    pub out: &'a Path,
    pub checkpoint_every: Option<u64>,
    pub port: u16,
    pub linger_seconds: u64,
    pub start_paused: bool,
}

#[derive(Default)]
struct Snapshot {
    tick: u64,
    site_stock: Vec<u64>,
    counts: Vec<u32>,
    final_hash: Option<String>,
}

struct Control {
    paused: bool,
    /// Single-step permits granted while paused.
    steps: u64,
}

struct Shared {
    snapshot: Mutex<Snapshot>,
    control: Mutex<Control>,
    wake: Condvar,
    shutdown: AtomicBool,
    meta: serde_json::Value,
    k: usize,
    n_sites: usize,
}

fn bind(port: u16) -> Result<Server, LabError> {
    Server::http((BIND_ADDRESS, port)).map_err(|e| LabError::Io(format!("cannot listen on port {port}: {e}")))
}

/// Runs the session while serving the viewer, then writes the same outputs as `lab run`.
pub fn view(session: &mut Session, options: &ViewOptions) -> Result<[u8; 32], LabError> {
    let server = bind(options.port)?;
    let end_tick = session.state.tick.saturating_add(options.ticks);
    let shared = Arc::new(Shared {
        snapshot: Mutex::new(Snapshot::default()),
        control: Mutex::new(Control { paused: options.start_paused, steps: 0 }),
        wake: Condvar::new(),
        shutdown: AtomicBool::new(false),
        meta: json!({
            "width": session.cfg.world.width,
            "height": session.cfg.world.height,
            "resources": session.cfg.world.resources,
            "ticks_total": end_tick,
        }),
        k: session.state.k,
        n_sites: session.state.topo.n_sites() as usize,
    });
    eprintln!("viewer: http://{BIND_ADDRESS}:{}/", options.port);

    let server_shared = Arc::clone(&shared);
    let server_thread = std::thread::spawn(move || serve(&server, &server_shared));

    let hook_shared = Arc::clone(&shared);
    let result = execute(session, options.ticks, options.out, options.checkpoint_every, |state| {
        publish(&hook_shared, state);
        if state.tick < end_tick {
            wait_for_permission(&hook_shared);
        }
    });
    if let Ok(hash) = &result {
        shared.snapshot.lock().expect("snapshot lock").final_hash = Some(hex(hash));
        println!("final tick={} hash={}", session.state.tick, hex(hash));
        std::thread::sleep(Duration::from_secs(options.linger_seconds));
    }
    shared.shutdown.store(true, Ordering::SeqCst);
    let _ = server_thread.join();
    result
}

fn publish(shared: &Shared, state: &State) {
    let mut snapshot = shared.snapshot.lock().expect("snapshot lock");
    snapshot.tick = state.tick;
    snapshot.site_stock.clone_from(&state.site_stock);
    snapshot.counts.clear();
    snapshot.counts.resize(shared.n_sites, 0);
    for entity in &state.entities {
        snapshot.counts[entity.site as usize] += 1;
    }
}

/// Blocks while paused, unless a single-step permit is available.
fn wait_for_permission(shared: &Shared) {
    let mut control = shared.control.lock().expect("control lock");
    while control.paused {
        if control.steps > 0 {
            control.steps -= 1;
            return;
        }
        control = shared.wake.wait(control).expect("control lock");
    }
}

fn serve(server: &Server, shared: &Shared) {
    while !shared.shutdown.load(Ordering::SeqCst) {
        if let Ok(Some(request)) = server.recv_timeout(Duration::from_millis(50)) {
            handle(request, shared);
        }
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header is valid")
}

fn handle(mut request: Request, shared: &Shared) {
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let response = match (request.method(), path) {
        (Method::Get, "/") => Response::from_string(PAGE).with_header(header("Content-Type", "text/html; charset=utf-8")),
        (Method::Get, "/meta") => {
            let snapshot = shared.snapshot.lock().expect("snapshot lock");
            let paused = shared.control.lock().expect("control lock").paused;
            let mut meta = shared.meta.clone();
            meta["tick"] = json!(snapshot.tick);
            meta["paused"] = json!(paused);
            meta["done"] = json!(snapshot.final_hash.is_some());
            meta["final_hash"] = json!(snapshot.final_hash);
            Response::from_string(meta.to_string()).with_header(header("Content-Type", "application/json"))
        }
        (Method::Get, "/frame") => match query.strip_prefix("resource=").and_then(|v| v.parse::<usize>().ok()).filter(|&r| r < shared.k) {
            Some(resource) => Response::from_data(frame(shared, resource)).with_header(header("Content-Type", "application/octet-stream")),
            None => Response::from_string("resource must be a valid resource index").with_status_code(400),
        },
        (Method::Post, "/control") => {
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);
            let mut control = shared.control.lock().expect("control lock");
            let known = match body.trim() {
                "pause" => {
                    control.paused = true;
                    true
                }
                "resume" => {
                    control.paused = false;
                    control.steps = 0;
                    true
                }
                "step" => {
                    control.steps += 1;
                    true
                }
                _ => false,
            };
            drop(control);
            shared.wake.notify_all();
            if known {
                Response::from_string("ok")
            } else {
                Response::from_string("control must be pause, resume or step").with_status_code(400)
            }
        }
        _ => Response::from_string("not found").with_status_code(404),
    };
    let _ = request.respond(response.with_header(header("Cache-Control", "no-store")));
}

/// `tick u64`, then per site the stock of `resource` saturated to `u32`, then per site the entity count; little-endian.
fn frame(shared: &Shared, resource: usize) -> Vec<u8> {
    let snapshot = shared.snapshot.lock().expect("snapshot lock");
    let mut bytes = Vec::with_capacity(8 + 8 * shared.n_sites);
    bytes.extend_from_slice(&snapshot.tick.to_le_bytes());
    for site in snapshot.site_stock.chunks_exact(shared.k) {
        bytes.extend_from_slice(&u32::try_from(site[resource]).unwrap_or(u32::MAX).to_le_bytes());
    }
    for count in &snapshot.counts {
        bytes.extend_from_slice(&count.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    #[test]
    fn binds_loopback_only() {
        let server = super::bind(0).unwrap();
        assert!(server.server_addr().to_ip().unwrap().ip().is_loopback());
    }
}
