pub mod analyzer;
pub mod knowledge;
pub mod materializer;
pub mod milestone_pattern;
pub mod planner;
pub mod prd;
pub mod prd_fetcher;
pub mod prd_source;
mod prompts;
pub mod scaffolder;
pub mod scanner;
pub mod suggestions;
pub mod types;

/// Configuration for the `maestro adapt` command.
#[derive(Debug, Clone, PartialEq)]
pub struct AdaptConfig {
    pub path: std::path::PathBuf,
    pub dry_run: bool,
    pub no_issues: bool,
    pub scan_only: bool,
    pub model: Option<String>,
    pub prd_source: prd_source::PrdSource,
}

impl Default for AdaptConfig {
    fn default() -> Self {
        Self {
            path: std::path::PathBuf::from("."),
            dry_run: false,
            no_issues: false,
            scan_only: false,
            model: None,
            prd_source: prd_source::PrdSource::default(),
        }
    }
}

/// Configuration for the `maestro prd` standalone command.
#[derive(Debug, Clone, PartialEq)]
pub struct PrdConfig {
    pub path: std::path::PathBuf,
    pub model: Option<String>,
    pub force: bool,
    pub source: prd_source::PrdSource,
}

pub async fn cmd_prd(config: PrdConfig) -> anyhow::Result<()> {
    use analyzer::{ClaudeAnalyzer, ProjectAnalyzer};
    use prd::{ClaudePrdGenerator, PrdGenerator};
    use scanner::{LocalProjectScanner, ProjectScanner};

    let output_path = config.path.join("docs/PRD.md");

    // When the source is Local, preserve legacy --force behavior so we don't
    // accidentally overwrite a hand-edited file without a flag.
    if config.source == prd_source::PrdSource::Local && output_path.exists() && !config.force {
        eprintln!(
            "PRD already exists at {}. Use --force to overwrite.",
            output_path.display()
        );
        return Ok(());
    }

    let model = config.model.as_deref().unwrap_or("sonnet").to_string();

    eprintln!("Scanning project...");
    let scanner = LocalProjectScanner::new();
    let profile = scanner.scan(&config.path).await?;

    eprintln!("Analyzing project...");
    let analyzer = ClaudeAnalyzer::new(model.clone());
    let report = analyzer.analyze(&profile).await?;

    // Try to fetch an existing PRD from the selected source. If found, we
    // ENRICH it; otherwise generate fresh.
    let existing = prd_fetcher::fetch_existing(config.source, &config.path).unwrap_or(None);
    if let Some(ref fetched) = existing {
        eprintln!(
            "Existing PRD found ({}) — enriching instead of regenerating.",
            fetched.origin.describe()
        );
    } else {
        eprintln!("No existing PRD found — generating from scratch.");
    }

    eprintln!("Generating PRD...");
    let generator = ClaudePrdGenerator::new(model);
    let prd_content = if let Some(fetched) = existing.as_ref() {
        generator
            .enrich(&profile, &report, &fetched.content)
            .await?
    } else {
        generator.generate(&profile, &report).await?
    };

    // Write back to the local file when the source includes it.
    if config.source.uses_local() {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&output_path, &prd_content)?;
        eprintln!("PRD written to {}", output_path.display());
    } else {
        // For remote-only sources, surface the result so the user can
        // copy it back manually until full write-back is implemented.
        eprintln!(
            "PRD content ({} chars) — paste into the selected destination:",
            prd_content.len()
        );
        println!("{}", prd_content);
    }

    Ok(())
}

pub async fn cmd_adapt(config: AdaptConfig) -> anyhow::Result<()> {
    use analyzer::{ClaudeAnalyzer, ProjectAnalyzer};
    use materializer::{GhMaterializer, PlanMaterializer};
    use planner::{AdaptPlanner, ClaudePlanner};
    use scanner::{LocalProjectScanner, ProjectScanner};

    let model = config.model.as_deref().unwrap_or("sonnet").to_string();

    // Phase 1: Scan
    eprintln!("Phase 1: Scanning project...");
    let scanner = LocalProjectScanner::new();
    let profile = scanner.scan(&config.path).await?;
    eprintln!(
        "  Language: {:?}, {} source files, {} lines",
        profile.language, profile.source_stats.total_files, profile.source_stats.total_lines
    );

    if config.scan_only {
        let json = serde_json::to_string_pretty(&profile)?;
        println!("{}", json);
        return Ok(());
    }

    // Phase 2: Analyze
    eprintln!("Phase 2: Analyzing with Claude...");
    let analyzer = ClaudeAnalyzer::new(model.clone());
    let report = match analyzer.analyze(&profile).await {
        Ok(r) => {
            eprintln!(
                "  {} modules, {} tech debt items",
                r.modules.len(),
                r.tech_debt_items.len()
            );
            r
        }
        Err(e) => {
            eprintln!("  Phase 2 failed: {}. Continuing with empty report.", e);
            types::AdaptReport {
                summary: String::new(),
                modules: vec![],
                tech_debt_items: vec![],
            }
        }
    };

    if config.no_issues {
        let json = serde_json::to_string_pretty(&report)?;
        println!("{}", json);
        return Ok(());
    }

    // Phase 2.5: Consolidate (PRD generation)
    eprintln!("Phase 2.5: Generating PRD...");
    use prd::PrdGenerator;
    let prd_generator = prd::ClaudePrdGenerator::new(model.clone());
    let prd_content = match prd_generator.generate(&profile, &report).await {
        Ok(content) => {
            let prd_path = config.path.join("docs/PRD.md");
            if !prd_path.exists() {
                if let Some(parent) = prd_path.parent()
                    && let Err(e) = std::fs::create_dir_all(parent)
                {
                    eprintln!("  Failed to create docs/: {}", e);
                }
                match std::fs::write(&prd_path, &content) {
                    Ok(()) => eprintln!("  PRD saved to {}", prd_path.display()),
                    Err(e) => eprintln!("  Failed to write PRD: {}", e),
                }
            } else {
                eprintln!("  PRD already exists, skipping write");
            }
            Some(content)
        }
        Err(e) => {
            eprintln!("  PRD generation failed: {}. Continuing without PRD.", e);
            None
        }
    };

    // Phase 2.6: Compress project knowledge into .maestro/knowledge.md so every
    // future session receives a dense, budget-bounded project brief.
    eprintln!("Phase 2.6: Compressing project knowledge...");
    let project_cfg = crate::config::Config::find_and_load_in(&config.path).ok();
    let knowledge_budget = project_cfg
        .as_ref()
        .map(|c| c.turboquant.knowledge_budget)
        .unwrap_or(4096);
    let tq_adapter = project_cfg.as_ref().and_then(build_adapter_from_config);
    let knowledge_base =
        knowledge::compress_knowledge(&profile, &report, tq_adapter.as_ref(), knowledge_budget);
    let knowledge_path = config.path.join(".maestro/knowledge.md");
    if let Some(parent) = knowledge_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        eprintln!("  Failed to create .maestro/: {}", e);
    }
    match std::fs::write(&knowledge_path, knowledge::to_markdown(&knowledge_base)) {
        Ok(()) => eprintln!(
            "  Knowledge base saved to {} ({} sections)",
            knowledge_path.display(),
            6
        ),
        Err(e) => eprintln!("  Failed to write knowledge.md: {}", e),
    }

    // Phase 2.7: Detect the repo's existing milestone-naming pattern so the
    // planner's output matches conventions already in use. Falls back to the
    // configured `MilestoneNaming` when the repo has no detectable pattern.
    let milestone_hint = detect_milestone_hint(&profile.root, project_cfg.as_ref()).await;
    if let Some(ref hint) = milestone_hint {
        let preview: String = hint.chars().take(120).collect();
        eprintln!("Phase 2.7: Milestone pattern → {}…", preview);
    } else {
        eprintln!("Phase 2.7: No milestone pattern detected (will defer to planner).");
    }

    // Phase 3: Plan
    eprintln!("Phase 3: Planning milestones and issues...");
    let planner = ClaudePlanner::new(model.clone());
    let plan = planner
        .plan(
            &profile,
            &report,
            prd_content.as_deref(),
            milestone_hint.as_deref(),
        )
        .await?;
    eprintln!(
        "  {} milestones, {} issues",
        plan.milestones.len(),
        plan.milestones
            .iter()
            .map(|m| m.issues.len())
            .sum::<usize>()
    );

    if config.dry_run {
        let json = serde_json::to_string_pretty(&plan)?;
        println!("{}", json);
        return Ok(());
    }

    // Phase 3.5: Scaffold
    eprintln!("Phase 3.5: Scaffolding .claude/ directory...");
    use scaffolder::{ClaudeScaffolder, ProjectScaffolder};
    let scaffolder = ClaudeScaffolder::new(model);
    match scaffolder.scaffold(&profile, &report, &plan).await {
        Ok(result) => {
            eprintln!(
                "  {} files created, {} skipped",
                result.created_count, result.skipped_count
            );
        }
        Err(e) => {
            eprintln!("  Scaffold failed: {}. Continuing without scaffolding.", e);
        }
    };

    // Phase 4: Materialize
    eprintln!("Phase 4: Creating GitHub artifacts...");
    let github = crate::provider::github::client::GhCliClient::new();
    let materializer = GhMaterializer::new(github);
    let result = materializer.materialize(&plan, &report, false).await?;

    eprintln!(
        "  Created {} milestones, {} issues",
        result.milestones_created.len(),
        result.issues_created.len()
    );
    if let Some(ref td) = result.tech_debt_issue {
        eprintln!("  Tech debt catalog: #{}", td.number);
    }

    Ok(())
}

/// Build the milestone-naming hint for the Claude planner.
///
/// Precedence:
/// 1. `MilestoneNaming::Custom` with a template → use it verbatim (user opt-in).
/// 2. `MilestoneNaming::Standard` or `Ai` → try to detect an existing pattern
///    from the repo's milestones. Return `Some(hint)` if detected, `None` if
///    the repo has no milestones or no dominant pattern (let Claude decide).
///
/// Failures talking to GitHub are non-fatal — returns `None` and adapt proceeds
/// with the planner's default behavior.
pub async fn detect_milestone_hint(
    _project_root: &std::path::Path,
    project_cfg: Option<&crate::config::Config>,
) -> Option<String> {
    use crate::config::MilestoneNaming;

    if let Some(cfg) = project_cfg
        && cfg.adapt.milestone_naming == MilestoneNaming::Custom
        && let Some(template) = cfg.adapt.milestone_template.as_deref()
    {
        return Some(format!(
            "Use this exact milestone title template (user-provided): `{}`. \
             `{{n}}` is the zero-based milestone index; `{{title}}` is a short description.",
            template
        ));
    }

    let github = crate::provider::github::client::GhCliClient::new();
    let mut titles: Vec<String> = Vec::new();
    for state in ["open", "closed"] {
        match crate::provider::github::client::GitHubClient::list_milestones(&github, state).await {
            Ok(ms) => titles.extend(ms.into_iter().map(|m| m.title)),
            Err(e) => {
                tracing::warn!("Failed to list {state} milestones for pattern detection: {e}");
            }
        }
    }
    if titles.is_empty() {
        return None;
    }
    let refs: Vec<&str> = titles.iter().map(|s| s.as_str()).collect();
    milestone_pattern::build_planner_hint(&refs)
}

fn build_adapter_from_config(
    cfg: &crate::config::Config,
) -> Option<crate::turboquant::adapter::TurboQuantAdapter> {
    if !cfg.turboquant.enabled {
        return None;
    }
    Some(crate::turboquant::adapter::TurboQuantAdapter::new(
        cfg.turboquant.bit_width,
    ))
}
