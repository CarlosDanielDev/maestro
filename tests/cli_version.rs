//! `maestro --version` must report the released version (#1336).
//!
//! clap reads `CARGO_PKG_VERSION` from the crate that defines `Cli`, which is
//! `maestro-cli` since #1307. v0.33.0 shipped printing `maestro 0.0.0`
//! because that crate did not inherit `[workspace.package].version`.

use std::process::Command;

#[test]
fn version_flag_prints_the_package_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_maestro"))
        .arg("--version")
        .output()
        .expect("spawn maestro --version");
    assert!(output.status.success(), "maestro --version failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        format!("maestro {}", env!("CARGO_PKG_VERSION")),
        "maestro --version must print the root package version"
    );
}
