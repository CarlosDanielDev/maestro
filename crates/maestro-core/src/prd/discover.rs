//! Discover the canonical PRD issue in a GitHub repo (#321).
//!
//! Discovery cascades through three strategies (most-specific first) so
//! a project that hasn't bothered to apply the `prd` label still gets
//! its PRD detected:
//!
//! 1. `gh issue list --label prd --state all --limit 1`
//! 2. `gh issue list --search "PRD: in:title" --state all --limit 1`
//! 3. `gh issue view 1` (last-resort fallback — many maestro-style projects
//!    pin their PRD as issue #1)
//!
//! All `gh` failures degrade to `Ok(None)` rather than bubbling — the
//! sync flow still works without an ingested PRD.

#![deny(clippy::unwrap_used)]
#![allow(dead_code)]

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPrd {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub source: DiscoverySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverySource {
    LabelPrd,
    TitleSearch,
    IssueOne,
    LocalFile,
    AzureWiki,
}

impl DiscoverySource {
    pub const fn label(self) -> &'static str {
        match self {
            Self::LabelPrd => "github:label:prd",
            Self::TitleSearch => "github:title-search",
            Self::IssueOne => "github:issue-1",
            Self::LocalFile => "local:docs/PRD.md",
            Self::AzureWiki => "azure:wiki",
        }
    }
}

/// Try each discovery strategy in order; return the first hit, `None` if
/// nothing matched. Convenience wrapper around `discover_all` for
/// callers that just want the primary candidate.
pub fn discover() -> Option<DiscoveredPrd> {
    discover_all().into_iter().next()
}

/// Run every discovery strategy and return ALL real PRD candidates
/// (most-canonical first). Used by the explore UI so the user can see
/// what was found across GitHub + local + Azure and pick a source.
///
/// Order:
///   1. `--label prd` (explicit author intent)
///   2. GitHub issue #1 if it looks PRD-shaped
///   3. Local `docs/PRD.md`
///   4. Azure DevOps wiki page `/PRD` (if `az` is available)
///   5. GitHub title-search with strict `PRD:` prefix (last resort)
///
/// Issue templates (Acceptance Criteria + Blocked By/Definition of Done)
/// are filtered at every step. Candidates are deduplicated by `(host,
/// number)`: title-search and issue-1 fallback often surface the same
/// GitHub issue twice — the explore UI showing both would be noise.
pub fn discover_all() -> Vec<DiscoveredPrd> {
    let mut out = Vec::new();
    let mut push_if_real = |candidate: Option<DiscoveredPrd>| {
        if let Some(c) = candidate.filter(is_real_prd)
            && !is_duplicate(&out, &c)
        {
            out.push(c);
        }
    };
    push_if_real(try_label_prd());
    push_if_real(try_issue_one());
    push_if_real(try_local_file());
    push_if_real(try_azure_wiki());
    push_if_real(try_title_search());
    out
}

/// Two candidates are duplicates when they're from the same host family
/// (GitHub vs local vs Azure) AND identify the same resource (issue
/// number for GitHub, body fingerprint for local/Azure where number=0).
fn is_duplicate(existing: &[DiscoveredPrd], candidate: &DiscoveredPrd) -> bool {
    existing.iter().any(|e| {
        // GitHub issues collide by number.
        if candidate.number > 0
            && e.number == candidate.number
            && matches!(
                (e.source, candidate.source),
                (DiscoverySource::LabelPrd, DiscoverySource::IssueOne)
                    | (DiscoverySource::LabelPrd, DiscoverySource::TitleSearch)
                    | (DiscoverySource::IssueOne, DiscoverySource::LabelPrd)
                    | (DiscoverySource::IssueOne, DiscoverySource::TitleSearch)
                    | (DiscoverySource::TitleSearch, DiscoverySource::LabelPrd)
                    | (DiscoverySource::TitleSearch, DiscoverySource::IssueOne)
            )
        {
            return true;
        }
        // Local/Azure collide by body content (rare — only if both
        // configured to read the same wiki page).
        candidate.number == 0
            && e.number == 0
            && e.source == candidate.source
            && e.body == candidate.body
    })
}

/// A discovered candidate is a "real PRD" only when its body is NOT
/// shaped like an issue-template body and at least passes `looks_like_prd`.
fn is_real_prd(p: &DiscoveredPrd) -> bool {
    !looks_like_issue_template(&p.body) && looks_like_prd(&p.title, &p.body)
}

#[derive(Deserialize)]
struct GhIssueListItem {
    number: u64,
    title: String,
    body: String,
}

fn try_label_prd() -> Option<DiscoveredPrd> {
    let items = run_gh_list(&["--label", "prd", "--state", "all", "--limit", "1"])?;
    items.into_iter().next().map(|i| DiscoveredPrd {
        number: i.number,
        title: i.title,
        body: i.body,
        source: DiscoverySource::LabelPrd,
    })
}

fn try_title_search() -> Option<DiscoveredPrd> {
    // Search GitHub broadly, then enforce a strict `PRD:` (or `PRD-` /
    // `PRD —`) prefix locally — `gh issue list --search "PRD: in:title"`
    // matches any title containing "PRD" as a word, which would pull
    // unrelated issues like `feat: PRD flow — …`.
    let items = run_gh_list(&["--search", "PRD in:title", "--state", "all", "--limit", "5"])?;
    items
        .into_iter()
        .find(|i| has_strict_prd_title_prefix(&i.title))
        .map(|i| DiscoveredPrd {
            number: i.number,
            title: i.title,
            body: i.body,
            source: DiscoverySource::TitleSearch,
        })
}

/// True only when `title` starts with `PRD:`, `PRD-`, `PRD —`, `PRD –`,
/// or `PRD ` followed by another keyword. Whitespace and case-insensitive.
pub(crate) fn has_strict_prd_title_prefix(title: &str) -> bool {
    let t = title.trim().to_lowercase();
    t.starts_with("prd:")
        || t.starts_with("prd-")
        || t.starts_with("prd —")
        || t.starts_with("prd –")
        || t.starts_with("prd ")
        || t == "prd"
}

fn try_issue_one() -> Option<DiscoveredPrd> {
    let output = std::process::Command::new("gh")
        .args(["issue", "view", "1", "--json", "number,title,body"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let item: GhIssueListItem = serde_json::from_str(&stdout).ok()?;
    if !looks_like_prd(&item.title, &item.body) {
        return None;
    }
    Some(DiscoveredPrd {
        number: item.number,
        title: item.title,
        body: item.body,
        source: DiscoverySource::IssueOne,
    })
}

fn run_gh_list(extra_args: &[&str]) -> Option<Vec<GhIssueListItem>> {
    let mut args: Vec<&str> = vec!["issue", "list", "--json", "number,title,body"];
    args.extend(extra_args);
    let output = std::process::Command::new("gh").args(&args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).ok()
}

/// Hard cap on local PRD file size to prevent a runaway 50 MB
/// `docs/PRD.md` from blocking the discovery thread + ballooning App
/// memory. 512 KiB is generous for a real PRD.
const MAX_LOCAL_PRD_BYTES: u64 = 512 * 1024;

/// Read a `docs/PRD.md` file from the current working directory.
fn try_local_file() -> Option<DiscoveredPrd> {
    let path = std::env::current_dir().ok()?.join("docs/PRD.md");
    let metadata = std::fs::metadata(&path).ok()?;
    if metadata.len() > MAX_LOCAL_PRD_BYTES {
        tracing::warn!(
            path = %path.display(),
            size = metadata.len(),
            "skipping local PRD file: exceeds 512 KiB cap"
        );
        return None;
    }
    let body = std::fs::read_to_string(&path).ok()?;
    if body.trim().is_empty() {
        return None;
    }
    let title = body
        .lines()
        .find(|l| l.starts_with("# "))
        .and_then(|l| l.strip_prefix("# "))
        .unwrap_or("docs/PRD.md")
        .to_string();
    Some(DiscoveredPrd {
        number: 0, // local file has no issue number
        title,
        body,
        source: DiscoverySource::LocalFile,
    })
}

/// Best-effort Azure DevOps wiki fetch via `az devops wiki page show
/// --path /PRD`. Degrades to None if `az` isn't installed/authed.
fn try_azure_wiki() -> Option<DiscoveredPrd> {
    let output = std::process::Command::new("az")
        .args([
            "devops", "wiki", "page", "show", "--path", "/PRD", "--output", "json",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let page: AzureWikiPage = serde_json::from_str(&stdout).ok()?;
    if page.content.trim().is_empty() {
        return None;
    }
    let title = page.path.unwrap_or_else(|| "/PRD".into());
    Some(DiscoveredPrd {
        number: 0,
        title,
        body: page.content,
        source: DiscoverySource::AzureWiki,
    })
}

#[derive(serde::Deserialize)]
struct AzureWikiPage {
    content: String,
    #[serde(default)]
    path: Option<String>,
}

/// Heuristic: an issue is "PRD-like" when its title starts with `PRD:`
/// or its body has at least one of the canonical PRD section headings.
/// Used only by the issue-#1 fallback to avoid pulling random issues.
pub(crate) fn looks_like_prd(title: &str, body: &str) -> bool {
    if has_strict_prd_title_prefix(title) {
        return true;
    }
    let body_lower = body.to_lowercase();
    let prd_markers = [
        "## vision",
        "## mission",
        "## goals",
        "## non-goals",
        "## success criteria",
    ];
    prd_markers.iter().any(|m| body_lower.contains(m))
}

/// True when the body is shaped like a GitHub issue body, not a PRD —
/// has both `## Acceptance Criteria` and one of `## Blocked By` /
/// `## Definition of Done`. Issue templates use this exact combination.
pub(crate) fn looks_like_issue_template(body: &str) -> bool {
    let lower = body.to_lowercase();
    let has_ac = lower.contains("## acceptance criteria");
    let has_blocked_by = lower.contains("## blocked by");
    let has_dod = lower.contains("## definition of done");
    has_ac && (has_blocked_by || has_dod)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_prd_matches_prd_colon_title() {
        assert!(looks_like_prd("PRD: foo", ""));
        assert!(looks_like_prd("PRD foo", ""));
        assert!(looks_like_prd("PRD-foo", ""));
        assert!(looks_like_prd("prd:", ""));
    }

    #[test]
    fn looks_like_prd_matches_prd_specific_section_headings() {
        assert!(looks_like_prd("Random Title", "## Vision\n\nText"));
        assert!(looks_like_prd("Random Title", "## Mission\n\nText"));
        assert!(looks_like_prd("Random Title", "## Goals\n\n- a"));
        assert!(looks_like_prd("Random Title", "## Non-Goals\n\n- a"));
        assert!(looks_like_prd("Random Title", "## Success Criteria\n\n- a"));
    }

    #[test]
    fn looks_like_prd_does_not_match_acceptance_criteria_alone() {
        // `## Acceptance Criteria` belongs to issue templates, not PRDs.
        // Must NOT classify as PRD on its own — that was the bug that
        // made us ingest issue #321 instead of #1.
        assert!(!looks_like_prd(
            "Some issue title",
            "## Acceptance Criteria\n\n- [ ] x"
        ));
    }

    #[test]
    fn looks_like_prd_rejects_generic_issues() {
        assert!(!looks_like_prd(
            "Fix bug in parser",
            "Steps to reproduce..."
        ));
        assert!(!looks_like_prd("", ""));
    }

    #[test]
    fn discovery_source_labels_are_stable_and_namespaced() {
        // Labels are surfaced in the activity log + explore UI, so the
        // user-visible namespace prefix matters: GitHub vs local vs Azure.
        assert_eq!(DiscoverySource::LabelPrd.label(), "github:label:prd");
        assert_eq!(DiscoverySource::TitleSearch.label(), "github:title-search");
        assert_eq!(DiscoverySource::IssueOne.label(), "github:issue-1");
        assert_eq!(DiscoverySource::LocalFile.label(), "local:docs/PRD.md");
        assert_eq!(DiscoverySource::AzureWiki.label(), "azure:wiki");
    }

    #[test]
    fn strict_prd_prefix_accepts_canonical_titles() {
        assert!(has_strict_prd_title_prefix("PRD: Maestro"));
        assert!(has_strict_prd_title_prefix("PRD - foo"));
        assert!(has_strict_prd_title_prefix("PRD — foo"));
        assert!(has_strict_prd_title_prefix("PRD foo"));
        assert!(has_strict_prd_title_prefix("prd: lower"));
        assert!(has_strict_prd_title_prefix("PRD"));
    }

    #[test]
    fn strict_prd_prefix_rejects_meta_titles() {
        // The bug we just fixed: `feat: interactive PRD flow` was matching
        // because gh search treats "PRD" as a word anywhere in the title.
        assert!(!has_strict_prd_title_prefix(
            "feat: interactive PRD flow — live document"
        ));
        assert!(!has_strict_prd_title_prefix("Add PRD support"));
        assert!(!has_strict_prd_title_prefix("Refactor the PRD parser"));
    }

    #[test]
    fn issue_template_detected_when_ac_and_blocked_by() {
        let body =
            "## Overview\n\nx\n\n## Acceptance Criteria\n\n- [ ] x\n\n## Blocked By\n\n- None";
        assert!(looks_like_issue_template(body));
    }

    #[test]
    fn issue_template_detected_when_ac_and_definition_of_done() {
        let body = "## Acceptance Criteria\n\n- [ ] x\n\n## Definition of Done\n\n- [ ] x";
        assert!(looks_like_issue_template(body));
    }

    #[test]
    fn issue_template_not_detected_for_real_prd() {
        let body = "## Vision\n\nThe pitch.\n\n## Goals\n\n- a\n\n## Non-Goals\n\n- b\n\n## Success Criteria\n\n- c";
        assert!(!looks_like_issue_template(body));
    }

    #[test]
    fn looks_like_prd_no_longer_treats_overview_alone_as_prd() {
        // `## Overview` is part of every issue template — it must not
        // single-handedly classify a body as a PRD.
        let issue_body = "## Overview\n\nFix the parser bug.";
        assert!(!looks_like_prd("Fix parser bug", issue_body));
    }

    #[test]
    fn looks_like_prd_accepts_real_prd_section_combos() {
        let prd_body = "## Vision\n\nx\n\n## Goals\n\n- a";
        assert!(looks_like_prd("Some title", prd_body));
    }

    #[test]
    fn is_real_prd_skips_issue_templates() {
        let template = DiscoveredPrd {
            number: 321,
            title: "feat: PRD".into(),
            body: "## Acceptance Criteria\n- x\n## Blocked By\n- None".into(),
            source: DiscoverySource::TitleSearch,
        };
        assert!(!is_real_prd(&template));
    }

    #[test]
    fn is_real_prd_accepts_canonical_prd() {
        let real = DiscoveredPrd {
            number: 1,
            title: "PRD: Maestro".into(),
            body: "## Vision\n\nThe pitch.\n\n## Success Criteria\n\n- a".into(),
            source: DiscoverySource::IssueOne,
        };
        assert!(is_real_prd(&real));
    }
}
