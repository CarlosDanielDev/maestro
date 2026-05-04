use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal,
};
use regex::Regex;
use std::io::{IsTerminal, Write};
use std::path::Path;

use crate::init::{
    FsProjectDetector, ProjectDetector, RenderOutcome, render_or_merge,
    render_or_merge_with_provider, template::ProviderTemplate, walk::find_project_root,
};
use crate::provider::{detect_provider_from_remote, types::ProviderKind};

/// Public entry point used by the CLI. Forwards to [`cmd_init_inner`]
/// against the real filesystem and converts the logical exit code into
/// either `Ok(())` (success) or a process-exit (failure).
pub fn cmd_init(reset: bool, non_interactive: bool) -> Result<()> {
    let cwd = std::env::current_dir().context("reading current working directory")?;
    let root = find_project_root(&cwd);
    let detector = FsProjectDetector::new();
    let code =
        if non_interactive || !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
            cmd_init_inner(reset, &root, &detector)?
        } else {
            let remote_output = git_remote_verbose(&root)?;
            let remote_url = first_origin_remote_url(&remote_output);
            let mut prompter = TerminalInitPrompter;
            cmd_init_inner_with_options(
                reset,
                &root,
                &detector,
                InitOptions::interactive(remote_url.as_deref(), &mut prompter),
            )?
        };
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

pub trait InitPrompter {
    fn choose_provider(&mut self, detected: ProviderKind) -> Result<ProviderKind>;
    fn prompt_azdo_organization(&mut self) -> Result<String>;
    fn prompt_azdo_project(&mut self) -> Result<String>;
}

pub enum InitOptions<'a> {
    NonInteractive,
    Interactive {
        remote_url: Option<&'a str>,
        prompter: &'a mut dyn InitPrompter,
    },
}

impl<'a> InitOptions<'a> {
    pub fn non_interactive() -> Self {
        Self::NonInteractive
    }

    pub fn interactive(remote_url: Option<&'a str>, prompter: &'a mut dyn InitPrompter) -> Self {
        Self::Interactive {
            remote_url,
            prompter,
        }
    }
}

/// Pure orchestration helper: writes (or merges) `maestro.toml` and
/// returns the logical exit code. Tests drive this directly with a
/// `FakeProjectDetector` and a `tempfile::TempDir`.
pub fn cmd_init_inner(
    reset: bool,
    project_root: &Path,
    detector: &dyn ProjectDetector,
) -> Result<i32> {
    cmd_init_inner_with_options(
        reset,
        project_root,
        detector,
        InitOptions::non_interactive(),
    )
}

pub fn cmd_init_inner_with_options(
    reset: bool,
    project_root: &Path,
    detector: &dyn ProjectDetector,
    options: InitOptions<'_>,
) -> Result<i32> {
    let target = project_root.join("maestro.toml");

    if target.exists() && !reset {
        eprintln!(
            "maestro.toml already exists at {}. Use --reset to refresh detection.",
            target.display()
        );
        return Ok(2);
    }

    let existing = if reset && target.exists() {
        Some(
            std::fs::read_to_string(&target)
                .with_context(|| format!("reading existing {}", target.display()))?,
        )
    } else {
        None
    };

    let provider = provider_template_from_options(options)?;
    let outcome = if provider == ProviderTemplate::github() {
        render_or_merge(detector, project_root, existing.as_deref())?
    } else {
        render_or_merge_with_provider(detector, project_root, existing.as_deref(), &provider)?
    };

    match outcome {
        RenderOutcome::Fresh { stacks, content } => {
            // create_new: atomically reject if the file appeared after
            // our pre-check, instead of silently clobbering.
            match std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&target)
            {
                Ok(mut f) => f
                    .write_all(content.as_bytes())
                    .with_context(|| format!("writing {}", target.display()))?,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    eprintln!(
                        "maestro.toml already exists at {}. Use --reset to refresh detection.",
                        target.display()
                    );
                    return Ok(2);
                }
                Err(e) => {
                    return Err(
                        anyhow::Error::from(e).context(format!("writing {}", target.display()))
                    );
                }
            }
            if stacks.is_empty() {
                eprintln!(
                    "Warning: no project markers detected. Wrote a generic template at {}; \
                     fill in build/test/run commands manually.",
                    target.display()
                );
            } else {
                let names: Vec<&str> = stacks.iter().map(|s| s.id()).collect();
                println!(
                    "Detected: {}. Created {}",
                    names.join(", "),
                    target.display()
                );
            }
        }
        RenderOutcome::Merged { stacks, report } => {
            std::fs::write(&target, &report.merged_toml)
                .with_context(|| format!("writing {}", target.display()))?;
            let names: Vec<&str> = if stacks.is_empty() {
                vec!["none"]
            } else {
                stacks.iter().map(|s| s.id()).collect()
            };
            println!(
                "Reset complete: detected {}, added {} key(s), preserved {} customized key(s).",
                names.join(", "),
                report.keys_added.len(),
                report.keys_preserved.len()
            );
        }
    }

    Ok(0)
}

fn provider_template_from_options(options: InitOptions<'_>) -> Result<ProviderTemplate> {
    match options {
        InitOptions::NonInteractive => Ok(ProviderTemplate::github()),
        InitOptions::Interactive {
            remote_url,
            prompter,
        } => {
            let detected = remote_url
                .map(detect_provider_from_remote)
                .unwrap_or(ProviderKind::Github);
            let selected = prompter.choose_provider(detected)?;
            match selected {
                ProviderKind::Github => Ok(ProviderTemplate::github()),
                ProviderKind::AzureDevops => {
                    let (organization, az_project) = prompt_azdo_fields(prompter)?;
                    Ok(ProviderTemplate::azure_devops(organization, az_project))
                }
            }
        }
    }
}

pub fn first_origin_remote_url(remote_verbose: &str) -> Option<String> {
    let mut first_origin = None;
    for line in remote_verbose.lines() {
        let mut parts = line.split_whitespace();
        let name = parts.next();
        let url = parts.next();
        let kind = parts.next();
        if name != Some("origin") {
            continue;
        }
        let Some(url) = url else {
            continue;
        };
        if first_origin.is_none() {
            first_origin = Some(url.to_string());
        }
        if kind == Some("(fetch)") {
            return Some(url.to_string());
        }
    }
    first_origin
}

pub fn validate_azure_devops_organization_url(input: &str) -> bool {
    let dev_azure = Regex::new(r"^https://dev\.azure\.com/[^/]+$").expect("valid regex");
    let visualstudio = Regex::new(r"^https://[^/]+\.visualstudio\.com$").expect("valid regex");
    !input.chars().any(char::is_control)
        && (dev_azure.is_match(input) || visualstudio.is_match(input))
}

pub fn validate_azure_devops_project(input: &str) -> bool {
    !input.trim().is_empty() && !input.chars().any(char::is_control)
}

pub fn prompt_azdo_fields(prompter: &mut dyn InitPrompter) -> Result<(String, String)> {
    let organization = loop {
        let value = prompter.prompt_azdo_organization()?;
        let trimmed = value.trim();
        if validate_azure_devops_organization_url(trimmed) {
            break trimmed.to_string();
        }
        eprintln!(
            "Azure DevOps organization must be https://dev.azure.com/<org> or https://<org>.visualstudio.com"
        );
    };

    let az_project = loop {
        let value = prompter.prompt_azdo_project()?;
        let trimmed = value.trim();
        if validate_azure_devops_project(trimmed) {
            break trimmed.to_string();
        }
        eprintln!("Azure DevOps project is required.");
    };

    Ok((organization, az_project))
}

fn git_remote_verbose(project_root: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(project_root)
        .arg("remote")
        .arg("-v")
        .output()
        .with_context(|| format!("running git remote -v in {}", project_root.display()))?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

struct TerminalInitPrompter;

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

#[cfg(test)]
#[path = "init_tests.rs"]
mod tests;
