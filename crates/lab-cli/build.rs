//! Records the exact code version and toolchain in the binary, for run manifests.
use std::process::Command;

fn capture(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    let commit = capture("git", &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = capture("git", &["status", "--porcelain"]).map(|s| !s.is_empty());
    let rustc = std::env::var("RUSTC").ok().and_then(|r| capture(&r, &["--version"])).unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=LAB_GIT_COMMIT={commit}");
    println!("cargo:rustc-env=LAB_GIT_DIRTY={}", dirty.map_or("unknown".to_string(), |d| d.to_string()));
    println!("cargo:rustc-env=LAB_RUSTC={rustc}");
    println!("cargo:rustc-env=LAB_TARGET={}", std::env::var("TARGET").unwrap_or_else(|_| "unknown".into()));
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");
}
