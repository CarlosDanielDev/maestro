//! Raw-mode terminal prompts for interactive `maestro init` (crossterm).
//!
//! Lives in the binary, not `maestro-cli`, so the CLI crate stays free of
//! terminal-UI dependencies (#1307); `main` passes it to `cmd_init`.

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal,
};
use maestro_cli::commands::InitPrompter;
use maestro_core::provider::types::ProviderKind;
use std::io::Write;

pub(crate) struct TerminalInitPrompter;

impl InitPrompter for TerminalInitPrompter {
    fn choose_provider(&mut self, detected: ProviderKind) -> Result<ProviderKind> {
        let default_key = match detected {
            ProviderKind::Github => "G",
            ProviderKind::AzureDevops => "A",
        };
        eprintln!(
            "Provider [{}]: press Enter for detected default, g for GitHub, a for Azure DevOps",
            default_key
        );
        loop {
            let key = read_key()?;
            match key.code {
                KeyCode::Enter => return Ok(detected),
                KeyCode::Char('g') | KeyCode::Char('G') => return Ok(ProviderKind::Github),
                KeyCode::Char('a') | KeyCode::Char('A') => return Ok(ProviderKind::AzureDevops),
                KeyCode::Esc | KeyCode::Char('c')
                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    anyhow::bail!("maestro init aborted")
                }
                _ => eprintln!("Choose g, a, or Enter."),
            }
        }
    }

    fn prompt_azdo_organization(&mut self) -> Result<String> {
        prompt_line("Azure DevOps organization URL")
    }

    fn prompt_azdo_project(&mut self) -> Result<String> {
        prompt_line("Azure DevOps project")
    }
}

fn read_key() -> Result<KeyEvent> {
    let _raw = RawModeGuard::new()?;
    let result = loop {
        if let Event::Key(key) = event::read().context("reading terminal event")? {
            break key;
        }
    };
    Ok(result)
}

fn prompt_line(label: &str) -> Result<String> {
    eprint!("{label}: ");
    std::io::stderr().flush().context("flushing prompt")?;
    let _raw = RawModeGuard::new()?;
    let mut value = String::new();
    loop {
        if let Event::Key(key) = event::read().context("reading terminal event")? {
            match key.code {
                KeyCode::Enter => {
                    eprintln!();
                    return Ok(value);
                }
                KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('d')
                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    anyhow::bail!("maestro init aborted")
                }
                KeyCode::Backspace if value.pop().is_some() => {
                    eprint!("\u{8} \u{8}");
                    std::io::stderr().flush().context("flushing prompt")?;
                }
                KeyCode::Char(c) => {
                    value.push(c);
                    eprint!("{c}");
                    std::io::stderr().flush().context("flushing prompt")?;
                }
                _ => {}
            }
        }
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Result<Self> {
        terminal::enable_raw_mode().context("enabling raw mode")?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}
