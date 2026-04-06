// NOTE: cli.rs must remain self-contained (no imports from other src/ modules)
// because build.rs includes it directly via #[path].
#[path = "src/cli.rs"]
mod cli;

fn main() {
    use clap::CommandFactory;

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
