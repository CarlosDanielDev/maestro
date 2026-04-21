//! Fetch an existing PRD from the configured source(s) so the Consolidate
//! phase can enrich it rather than regenerate from scratch.
//!
//! Tech-stack-agnostic: GitHub is reached via `gh`, Azure DevOps via `az`.
//! All provider calls are best-effort — a missing or failed remote fetch
//! falls back to the local file (when using `Both`) or to "no existing
//! PRD" (treated as the legacy generate-fresh path).

use std::path::Path;

use super::prd_source::{FetchedPrd, PrdOrigin, PrdSource};

/// Attempt to fetch an existing PRD from the selected source(s).
///
/// Returns `Ok(None)` when no existing PRD is found — the caller should
/// generate fresh in that case. Returns `Err` only for hard failures
/// (provider invoked but crashed in an unexpected way).
pub fn fetch_existing(
    source: PrdSource,
    project_root: &Path,
) -> anyhow::Result<Option<FetchedPrd>> {
    match source {
        PrdSource::Local => fetch_local(project_root),
        PrdSource::Github => fetch_github(project_root),
        PrdSource::Azure => fetch_azure(project_root),
        PrdSource::Both => {
            // Prefer the remote content — it's the "upstream" — and fall back
            // to local if the remote fetch comes up empty.
            match fetch_github(project_root)? {
                Some(remote) => Ok(Some(merge_local_if_present(remote, project_root)?)),
                None => fetch_local(project_root),
            }
        }
    }
}

fn fetch_local(project_root: &Path) -> anyhow::Result<Option<FetchedPrd>> {
    let path = project_root.join("docs/PRD.md");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if content.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(FetchedPrd {
        content,
        origin: PrdOrigin::Local { path },
    }))
}

/// Try to find an existing PRD among GitHub issues in the current repo by
/// looking for an open issue labeled `prd`. Delegates repo detection to the
/// `gh` CLI (via `git remote`).
fn fetch_github(_project_root: &Path) -> anyhow::Result<Option<FetchedPrd>> {
    let labeled = std::process::Command::new("gh")
        .args([
            "issue",
            "list",
            "--label",
            "prd",
            "--limit",
            "1",
            "--state",
            "open",
            "--json",
            "number,title,body",
        ])
        .output();

    let Ok(output) = labeled else {
        // `gh` not installed or not on PATH — treat as "nothing found"
        // rather than a hard error so the flow falls through to generation.
        return Ok(None);
    };

    if !output.status.success() {
        // `gh` exited non-zero (not authed / no repo). Degrade gracefully.
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let items: Vec<GhIssueListItem> = match serde_json::from_str(&stdout) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };

    let Some(first) = items.into_iter().next() else {
        return Ok(None);
    };

    if first.body.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(FetchedPrd {
        content: first.body,
        origin: PrdOrigin::GithubIssue {
            number: first.number,
        },
    }))
}

/// Azure DevOps fetch — reads from a wiki page whose title is `PRD`.
///
/// Uses the `az` CLI (`az devops` + `az devops wiki page show`). Requires
/// the user to be signed in (`az login`) and to have set a default
/// organization/project (`az devops configure --defaults ...`).
///
/// This is a minimal implementation; we degrade to "no existing PRD" when
/// `az` is missing, fails, or returns empty output, rather than surfacing
/// errors to the adapt pipeline.
fn fetch_azure(_project_root: &Path) -> anyhow::Result<Option<FetchedPrd>> {
    let cmd = std::process::Command::new("az")
        .args([
            "devops", "wiki", "page", "show", "--path", "/PRD", "--output", "json",
        ])
        .output();

    let Ok(output) = cmd else {
        return Ok(None);
    };

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let page: AzureWikiPage = match serde_json::from_str(&stdout) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };

    if page.content.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(FetchedPrd {
        content: page.content,
        origin: PrdOrigin::AzureWiki {
            project: page.project.unwrap_or_else(|| "<default>".into()),
            page: page.path.unwrap_or_else(|| "/PRD".into()),
        },
    }))
}

/// When the user selects `Both`, concatenate the remote PRD and the local
/// file (if present) so the consolidation prompt sees both. We keep the
/// remote origin so write-back goes to the source of truth.
fn merge_local_if_present(remote: FetchedPrd, project_root: &Path) -> anyhow::Result<FetchedPrd> {
    let Some(local) = fetch_local(project_root)? else {
        return Ok(remote);
    };

    let merged = format!(
        "{remote_body}\n\n<!-- local addendum below ({local_desc}) -->\n\n{local_body}\n",
        remote_body = remote.content.trim_end(),
        local_desc = local.origin.describe(),
        local_body = local.content.trim(),
    );

    Ok(FetchedPrd {
        content: merged,
        origin: remote.origin,
    })
}

#[derive(serde::Deserialize)]
struct GhIssueListItem {
    number: u64,
    #[allow(dead_code)] // Reason: present for debugging / future display
    title: String,
    body: String,
}

#[derive(serde::Deserialize)]
struct AzureWikiPage {
    content: String,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_tmp_project<F: FnOnce(&Path)>(setup: F) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create tmp dir");
        setup(dir.path());
        dir
    }

    #[test]
    fn fetch_local_returns_none_when_no_prd_file() {
        let dir = with_tmp_project(|_| {});
        let res = fetch_local(dir.path()).unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn fetch_local_returns_some_when_prd_exists() {
        let dir = with_tmp_project(|p| {
            std::fs::create_dir_all(p.join("docs")).unwrap();
            std::fs::write(p.join("docs/PRD.md"), "# My PRD\n\nSome content").unwrap();
        });
        let res = fetch_local(dir.path()).unwrap();
        let prd = res.expect("should find local PRD");
        assert!(prd.content.contains("My PRD"));
        assert!(matches!(prd.origin, PrdOrigin::Local { .. }));
    }

    #[test]
    fn fetch_local_returns_none_for_empty_prd_file() {
        let dir = with_tmp_project(|p| {
            std::fs::create_dir_all(p.join("docs")).unwrap();
            std::fs::write(p.join("docs/PRD.md"), "   \n\n   ").unwrap();
        });
        let res = fetch_local(dir.path()).unwrap();
        assert!(res.is_none(), "whitespace-only file counts as absent");
    }

    #[test]
    fn fetch_existing_local_source_reads_local_file() {
        let dir = with_tmp_project(|p| {
            std::fs::create_dir_all(p.join("docs")).unwrap();
            std::fs::write(p.join("docs/PRD.md"), "# Local PRD").unwrap();
        });
        let res = fetch_existing(PrdSource::Local, dir.path()).unwrap();
        let prd = res.expect("should find local PRD");
        assert!(prd.content.contains("Local PRD"));
    }

    #[test]
    fn fetch_existing_local_source_returns_none_when_missing() {
        let dir = with_tmp_project(|_| {});
        let res = fetch_existing(PrdSource::Local, dir.path()).unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn fetch_existing_github_or_azure_degrades_to_none_without_provider() {
        // `gh` / `az` may or may not be installed on the test runner.
        // Either way, fetch_existing must not error — at worst it returns None.
        let dir = with_tmp_project(|_| {});
        let gh = fetch_existing(PrdSource::Github, dir.path());
        assert!(gh.is_ok(), "github fetch must not error: {:?}", gh.err());
        let az = fetch_existing(PrdSource::Azure, dir.path());
        assert!(az.is_ok(), "azure fetch must not error: {:?}", az.err());
    }

    #[test]
    fn fetch_existing_both_falls_back_to_local_when_remote_empty() {
        // Simulate: no `gh` available (or no matching issue) → None from
        // github fetch → falls through to local read.
        let dir = with_tmp_project(|p| {
            std::fs::create_dir_all(p.join("docs")).unwrap();
            std::fs::write(p.join("docs/PRD.md"), "# Local").unwrap();
        });
        let res = fetch_existing(PrdSource::Both, dir.path()).unwrap();
        // Either remote produced a value (rare in CI) or we fell back to local.
        let prd = res.expect("Both with local file present should yield something");
        assert!(!prd.content.trim().is_empty());
    }
}
