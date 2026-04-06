//! E2E smoke test for the Elixir backend via `tast run` CLI.
//!
//! Requires:
//! - `mix` on PATH (Elixir installed)
//! - The slugify repo cloned via `just platform-setup`
//!
//! Silently returns if prerequisites are missing so `cargo test` never
//! fails on machines without Elixir.

use std::path::PathBuf;
use std::process::Command;

fn mix_available() -> bool {
    Command::new("mix")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn slugify_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/platform/elixir/slugify")
}

fn slugify_cloned() -> bool {
    slugify_dir().join("mix.exs").exists()
}

fn ensure_deps() {
    let status = Command::new("mix")
        .args(["deps.get"])
        .current_dir(slugify_dir())
        .env("MIX_ENV", "test")
        .status()
        .expect("failed to run mix deps.get");
    assert!(status.success(), "mix deps.get failed");
}

fn tast_binary() -> PathBuf {
    // cargo test builds into target/debug
    let mut path = PathBuf::from(env!("CARGO_BIN_EXE_tast"));
    // Fallback: use cargo run if the binary isn't found
    if !path.exists() {
        path = PathBuf::from("target/debug/tast");
    }
    path
}

#[test]
fn e2e_tast_run_slugify() {
    if !mix_available() || !slugify_cloned() {
        return;
    }
    ensure_deps();

    let tast_file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/platform/elixir/slugify.tast");

    let output = Command::new(tast_binary())
        .args([
            "run",
            &tast_file.to_string_lossy(),
            "--backend",
            "elixir",
            "--working-dir",
            &slugify_dir().to_string_lossy(),
            "--keep-harness",
            "--timeout",
            "120",
        ])
        .output()
        .expect("failed to execute tast run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // tast run exits 1 when any step fails, which is expected for the
    // generated harness. What matters is that it executed (produced YAML
    // output) and didn't crash (no stderr panic/error before output).
    assert!(
        !stdout.is_empty(),
        "tast run should produce output.\nstderr:\n{stderr}"
    );

    // The YAML output should contain the graph name and step results.
    assert!(
        stdout.contains("SlugifyPipeline"),
        "output should reference the SlugifyPipeline graph.\nstdout:\n{stdout}"
    );

    // At least the first step (ParseOptions, no deps) should have executed.
    assert!(
        stdout.contains("ParseOptions"),
        "output should contain ParseOptions step.\nstdout:\n{stdout}"
    );
}
