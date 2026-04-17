//! Build script for maestro.
//!
//! **What it does:** Generates man pages and shell completions from the clap
//! CLI definition so they stay in sync with the actual command structure.
//!
//! **Outputs** (written to `$OUT_DIR/`):
//! - `man/maestro.1`            — roff man page
//! - `completions/maestro.bash` — Bash completions
//! - `completions/maestro.zsh`  — Zsh completions
//! - `completions/maestro.fish` — Fish completions
//!
//! **Re-runs when:**
//! - `build.rs` changes  (implicit, handled by Cargo)
//! - `src/cli.rs` changes (explicit directive below — this file defines the CLI)
//! - `Cargo.toml` changes (explicit directive below — version / dependency changes)

#![allow(clippy::expect_used)] // Build scripts conventionally use expect()
// NOTE: cli.rs must remain self-contained (no imports from other src/ modules)
// because build.rs includes it directly via #[path].
#[path = "src/cli.rs"]
mod cli;

fn main() {
    use clap::CommandFactory;

    // Tell Cargo when to re-run this script.
    println!("cargo:rerun-if-changed=src/cli.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");

    let out_dir =
        std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR must be set by cargo"));

    // Generate man page (reuses cli::cmd_mangen)
    let man_dir = out_dir.join("man");
    cli::cmd_mangen(&man_dir).expect("failed to generate man page");

    // Generate shell completions
    let comp_dir = out_dir.join("completions");
    std::fs::create_dir_all(&comp_dir).unwrap();
    for shell in [
        clap_complete::Shell::Bash,
        clap_complete::Shell::Zsh,
        clap_complete::Shell::Fish,
    ] {
        let mut cmd = cli::Cli::command();
        clap_complete::generate_to(shell, &mut cmd, "maestro", &comp_dir).unwrap();
    }
}
