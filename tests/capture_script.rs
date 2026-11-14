//! Demo capture script contract (#1061).
//!
//! `docs/assets/capture.sh` must stay re-runnable: it names its tooling,
//! carries install instructions, and writes the capture under `docs/assets/`
//! for the `gh-pages` hero embed, from a scripted agent on a throwaway HOME
//! (the real ~/.claude must never reach a public capture). These assertions
//! pin that contract so a
//! drive-by edit cannot silently drop the reproducibility story.

use std::fs;
use std::path::Path;

fn capture_script() -> String {
    let path = Path::new("docs/assets/capture.sh");
    fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "read {}: {e} — the scripted TUI capture must live here so it can be re-run when the UI changes",
            path.display()
        )
    })
}

#[test]
fn capture_script_names_tooling_with_install_instructions() {
    let script = capture_script();
    assert!(
        script.contains("vhs"),
        "script header must name the capture tooling (vhs)"
    );
    assert!(
        script.contains("brew install vhs"),
        "script must carry an install instruction for vhs"
    );
}

#[test]
fn capture_script_is_headless_and_writes_under_docs_assets() {
    let script = capture_script();
    assert!(
        script.to_lowercase().contains("headless"),
        "script must state it runs headless (no human keystrokes)"
    );
    assert!(
        script.contains("docs/assets/"),
        "capture output must land under docs/assets/ so it can be committed to gh-pages"
    );
}

#[test]
fn capture_script_runs_a_scripted_agent_for_deterministic_sessions() {
    let script = capture_script();
    assert!(
        script.contains("docs/assets/demo-agent.sh"),
        "sessions must come from the scripted demo agent so every capture shows the same run"
    );
    assert!(
        Path::new("docs/assets/demo-agent.sh").exists(),
        "the scripted demo agent must live next to the capture script"
    );
}

#[test]
fn capture_script_never_shows_the_real_home() {
    let script = capture_script();
    assert!(
        script.contains("HOME=\"$fake_home\""),
        "Fleet Board and Adopt Session read $HOME/.claude; the capture must run on a throwaway HOME"
    );
    assert!(
        script.contains("auto_pr = false") && script.contains("no-push://disabled"),
        "the capture must not be able to open PRs or push"
    );
}
