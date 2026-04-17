pub mod analyzer;
pub mod materializer;
pub mod planner;
mod prompts;
pub mod scanner;
pub mod types;

/// Configuration for the `maestro adapt` command.
#[derive(Debug, Clone, PartialEq)]
pub struct AdaptConfig {
    pub path: std::path::PathBuf,
    pub dry_run: bool,
    pub no_issues: bool,
    pub scan_only: bool,
    pub model: Option<String>,
}

impl Default for AdaptConfig {
    fn default() -> Self {
        Self {
            path: std::path::PathBuf::from("."),
            dry_run: false,
            no_issues: false,
            scan_only: false,
            model: None,
        }
    }
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

    // Phase 3: Plan
    eprintln!("Phase 3: Planning milestones and issues...");
    let planner = ClaudePlanner::new(model);
    let plan = planner.plan(&profile, &report).await?;
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
