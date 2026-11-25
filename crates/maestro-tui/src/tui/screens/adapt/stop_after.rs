//! Stop after (§02.11): one radio in place of the Dry Run / Scan Only /
//! No Issues toggles. It sets those same booleans, so the `complete_*`
//! chain and the CLI flags do not change.

use super::types::AdaptWizardConfig;

/// Where the adapt run stops. `Plan` is the default: a first run reads and
/// asks Claude, but writes nothing outside `.maestro/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StopAfter {
    Scan,
    Analysis,
    #[default]
    Plan,
    Full,
}

impl StopAfter {
    pub const ALL: [StopAfter; 4] = [Self::Scan, Self::Analysis, Self::Plan, Self::Full];

    /// Set the existing flags for this option.
    pub fn apply(&self, cfg: &mut AdaptWizardConfig) {
        cfg.stop_after = *self;
        let (scan_only, no_issues, dry_run) = match self {
            Self::Scan => (true, true, true),
            Self::Analysis => (false, true, true),
            Self::Plan => (false, false, true),
            Self::Full => (false, false, false),
        };
        cfg.scan_only = scan_only;
        cfg.no_issues = no_issues;
        cfg.dry_run = dry_run;
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Scan => "Scan",
            Self::Analysis => "Analysis",
            Self::Plan => "Plan · dry run",
            Self::Full => "Full run",
        }
    }

    /// One-line consequence drawn under the option.
    pub fn consequence(self) -> &'static str {
        match self {
            Self::Scan => "reads local files only, no Claude call",
            Self::Analysis => "adds 1 Claude call: modules and tech debt",
            Self::Plan => "adds PRD and plan, writes only the cache (recommended first run)",
            Self::Full => "writes files + GitHub items after the Review step",
        }
    }

    /// Claude calls this option makes (analysis, PRD, plan, scaffold).
    pub fn claude_calls(self) -> usize {
        match self {
            Self::Scan => 0,
            Self::Analysis => 1,
            Self::Plan => 3,
            Self::Full => 4,
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let i = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(i + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// "This run will": one `(heading, line)` per fact, from the code path
/// each option enables. `remote` is the provider name (GitHub, Azure DevOps).
pub fn run_plan(cfg: &AdaptWizardConfig, remote: &str) -> Vec<(&'static str, String)> {
    let stop = cfg.stop_after;
    let full = stop == StopAfter::Full;
    let mut read = vec![format!("local files under {}", cfg.path)];
    if matches!(stop, StopAfter::Plan | StopAfter::Full) {
        read.push(format!("{remote} milestone titles (plan hint)"));
    }
    if full {
        read.push(format!("{remote} labels"));
    }
    let send = match stop {
        StopAfter::Scan => "nothing".to_string(),
        StopAfter::Analysis => "1 call: project profile → modules, tech debt".to_string(),
        StopAfter::Plan => "3 calls: analysis, PRD draft, plan".to_string(),
        StopAfter::Full => "4 calls: analysis, PRD draft, plan, .claude/ files".to_string(),
    };
    let mut write = vec![".maestro/adapt-cache.json (resume cache)".to_string()];
    if full {
        write.push("docs/PRD.md if missing, after Review".to_string());
        write.push(".claude/ files, existing never overwritten".to_string());
    } else if stop == StopAfter::Plan {
        write.push("docs/PRD.md is drafted in memory, not written".to_string());
    }
    let create = if full {
        "labels, milestones, issues, tech-debt issue · after ⏎ ⏎ at Review".to_string()
    } else {
        "nothing".to_string()
    };
    let calls = stop.claude_calls();
    let cost = if calls == 0 {
        "no Claude call".to_string()
    } else {
        format!(
            "{calls} Claude calls on {} · not counted in the budget",
            cfg.model
        )
    };
    let mut out: Vec<(&'static str, String)> = Vec::new();
    out.extend(read.into_iter().map(|l| ("READ", l)));
    out.push(("SEND TO CLAUDE", send));
    out.extend(write.into_iter().map(|l| ("WRITE LOCALLY", l)));
    out.push(("CREATE ON REMOTE", create));
    out.push(("COST", cost));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_after_flags() {
        // (option, scan_only, no_issues, dry_run)
        let table = [
            (StopAfter::Scan, true, true, true),
            (StopAfter::Analysis, false, true, true),
            (StopAfter::Plan, false, false, true),
            (StopAfter::Full, false, false, false),
        ];
        for (stop, scan_only, no_issues, dry_run) in table {
            // Start from the opposite flags so every field must be set.
            let mut cfg = AdaptWizardConfig {
                scan_only: !scan_only,
                no_issues: !no_issues,
                dry_run: !dry_run,
                ..Default::default()
            };
            stop.apply(&mut cfg);
            assert_eq!(cfg.stop_after, stop, "{stop:?}");
            assert_eq!(
                (cfg.scan_only, cfg.no_issues, cfg.dry_run),
                (scan_only, no_issues, dry_run),
                "{stop:?}"
            );
        }
    }

    #[test]
    fn default_config_is_plan_dry_run() {
        let cfg = AdaptWizardConfig::default();
        assert_eq!(cfg.stop_after, StopAfter::Plan);
        assert!(cfg.dry_run, "a first run must not write");
        assert!(!cfg.scan_only && !cfg.no_issues);
    }

    #[test]
    fn next_and_prev_cycle_all_options() {
        let mut s = StopAfter::Scan;
        for _ in 0..4 {
            assert_eq!(s.next().prev(), s);
            s = s.next();
        }
        assert_eq!(s, StopAfter::Scan);
    }

    #[test]
    fn run_plan_creates_nothing_below_full() {
        for stop in [StopAfter::Scan, StopAfter::Analysis, StopAfter::Plan] {
            let mut cfg = AdaptWizardConfig::default();
            stop.apply(&mut cfg);
            let plan = run_plan(&cfg, "GitHub");
            let create: Vec<_> = plan
                .iter()
                .filter(|(h, _)| *h == "CREATE ON REMOTE")
                .collect();
            assert_eq!(
                create,
                [&("CREATE ON REMOTE", "nothing".to_string())],
                "{stop:?}"
            );
            let writes = plan
                .iter()
                .filter(|(h, l)| *h == "WRITE LOCALLY" && !l.contains("not written"));
            assert_eq!(writes.count(), 1, "{stop:?}: only the cache is written");
        }
    }
}
