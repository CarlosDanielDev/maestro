use anyhow::Result;
use std::io::IsTerminal;

use crate::config::Config;
use crate::config::trust::{TrustStore, content_hash, inventory};

/// `maestro trust` — record consent for this workspace's maestro config
/// (#1045). The record lives in the user's maestro config dir, keyed by
/// canonicalized config path + content hash, never inside the repo.
pub fn cmd_trust(yes: bool) -> Result<()> {
    let loaded = Config::find_and_load_with_path()?;
    let store = TrustStore::with_default_path()?;

    let findings = inventory(&loaded.config);
    if findings.is_empty() {
        println!(
            "This workspace's maestro config declares no executable commands; nothing to trust."
        );
        return Ok(());
    }

    println!("This workspace's maestro config declares executable commands:");
    for finding in &findings {
        println!("  {} = \"{}\"", finding.key, finding.value);
    }
    println!("Path: {}", loaded.path.display());

    if !yes {
        if !std::io::stdin().is_terminal() {
            anyhow::bail!(
                "Refusing to trust non-interactively. Re-run in a terminal to confirm, \
                 or pass --yes after reviewing the values above."
            );
        }
        println!("Trust this workspace's config? [y/N]");
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        let answer = answer.trim().to_ascii_lowercase();
        if answer != "y" && answer != "yes" {
            println!("Not trusted. Config-supplied commands stay skipped.");
            return Ok(());
        }
    }

    let content = std::fs::read_to_string(&loaded.path).unwrap_or_default();
    store.record(&loaded.path, &content_hash(&content))?;
    println!(
        "Workspace trusted. The record is stored outside this repo and is re-checked \
         whenever the config file changes."
    );
    Ok(())
}

/// `maestro untrust` — revoke consent for this workspace's config (#1045).
pub fn cmd_untrust() -> Result<()> {
    let loaded = Config::find_and_load_with_path()?;
    let store = TrustStore::with_default_path()?;

    if store.revoke(&loaded.path)? {
        println!("Trust revoked for {}.", loaded.path.display());
    } else {
        println!(
            "No trust record for {} — nothing to revoke.",
            loaded.path.display()
        );
    }
    Ok(())
}
