pub mod analyzer;
pub mod materializer;
pub mod planner;
pub mod prd;
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

/// Configuration for the `maestro prd` standalone command.
#[derive(Debug, Clone, PartialEq)]
pub struct PrdConfig {
    pub path: std::path::PathBuf,
    pub model: Option<String>,
    pub force: bool,
}

pub async fn cmd_prd(config: PrdConfig) -> anyhow::Result<()> {
    use prd::{ClaudePrdGenerator, PrdGenerator};
    use scanner::{LocalProjectScanner, ProjectScanner};
    use analyzer::{ClaudeAnalyzer, ProjectAnalyzer};

    let output_path = config.path.join("docs/PRD.md");
    if output_path.exists() && !config.force {
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

    eprintln!("Generating PRD...");
    let generator = ClaudePrdGenerator::new(model);
    let prd_content = generator.generate(&profile, &report).await?;

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output_path, &prd_content)?;
    eprintln!("PRD written to {}", output_path.display());

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

    // Phase 3: Plan
    eprintln!("Phase 3: Planning milestones and issues...");
    let planner = ClaudePlanner::new(model);
    let plan = planner.plan(&profile, &report, prd_content.as_deref()).await?;
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
