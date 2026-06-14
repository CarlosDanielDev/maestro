//! Heuristic feature↔module mapping and the [`compute_coverage`] roll-up.
//!
//! Mapping is intentionally low-confidence (last-path-segment name match); the
//! signal it produces is surfaced through `coverage.modules_orphaned`, not
//! treated as ground truth.

use crate::insight::features::kebab;
use crate::insight::schema::{Coverage, Feature, Module};

/// Link a feature to a module when the feature name (kebab-normalized) equals
/// the module path's last segment (also kebab-normalized). Writes both
/// directions: the module path into `feature.modules`, the feature id into
/// `module.feature_ids`. Low confidence by design.
pub(crate) fn map_modules(features: &mut [Feature], modules: &mut [Module]) {
    for feature in features.iter_mut() {
        let key = kebab(&feature.name);
        if key.is_empty() {
            continue;
        }
        for module in modules.iter_mut() {
            let last = module
                .path
                .rsplit('/')
                .next()
                .unwrap_or(module.path.as_str());
            if kebab(last) != key {
                continue;
            }
            if !feature.modules.contains(&module.path) {
                feature.modules.push(module.path.clone());
            }
            if !module.feature_ids.contains(&feature.id) {
                module.feature_ids.push(feature.id.clone());
            }
        }
    }
}

/// Roll up documentation coverage: total surfaces, how many carry a static
/// summary, and the sorted list of modules no feature mapped to.
pub(crate) fn compute_coverage(features: &[Feature], modules: &[Module]) -> Coverage {
    let surfaces_total = features.len() as u64;
    let surfaces_documented = features
        .iter()
        .filter(|f| !f.summary_static.trim().is_empty())
        .count() as u64;
    let mut modules_orphaned: Vec<String> = modules
        .iter()
        .filter(|m| m.feature_ids.is_empty())
        .map(|m| m.path.clone())
        .collect();
    modules_orphaned.sort();
    Coverage {
        surfaces_total,
        surfaces_documented,
        modules_orphaned,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insight::schema::SurfaceType;

    fn feature(id: &str, surface_type: SurfaceType, name: &str, summary: &str) -> Feature {
        Feature {
            id: id.into(),
            surface_type,
            name: name.into(),
            entry_points: vec![],
            modules: vec![],
            summary_static: summary.into(),
            behavior_narrative: None,
            since: None,
            related: vec![],
        }
    }

    #[test]
    fn map_modules_links_feature_and_module_on_last_segment_match() {
        let mut features = vec![feature("cli-run", SurfaceType::Cli, "run", "")];
        let mut modules = vec![Module {
            path: "src/run".into(),
            ..Module::default()
        }];

        map_modules(&mut features, &mut modules);

        assert!(
            features[0].modules.contains(&"src/run".to_string()),
            "feature.modules must contain the matched module path"
        );
        assert!(
            modules[0].feature_ids.contains(&"cli-run".to_string()),
            "module.feature_ids must contain the matched feature id"
        );
    }

    #[test]
    fn map_modules_leaves_unmatched_modules_unlinked() {
        let mut features = vec![feature("cli-run", SurfaceType::Cli, "run", "")];
        let mut modules = vec![Module {
            path: "src/session".into(),
            ..Module::default()
        }];

        map_modules(&mut features, &mut modules);

        assert!(
            features[0].modules.is_empty(),
            "unmatched feature must not gain any module"
        );
        assert!(
            modules[0].feature_ids.is_empty(),
            "unmatched module must not gain any feature id"
        );
    }

    #[test]
    fn compute_coverage_reports_orphaned_modules() {
        let features = vec![feature("cli-run", SurfaceType::Cli, "run", "Runs things.")];
        let modules = vec![Module {
            path: "src/session".into(),
            ..Module::default()
        }];

        let cov = compute_coverage(&features, &modules);

        assert!(
            cov.modules_orphaned.contains(&"src/session".to_string()),
            "unlinked module must appear in modules_orphaned"
        );
    }

    #[test]
    fn compute_coverage_counts_surfaces_and_documented() {
        let features = vec![
            feature("cli-run", SurfaceType::Cli, "run", "Runs things."),
            feature("cli-stop", SurfaceType::Cli, "stop", ""),
            feature(
                "tui-mode-home",
                SurfaceType::TuiMode,
                "home",
                "Home screen.",
            ),
        ];
        let modules: Vec<Module> = vec![];

        let cov = compute_coverage(&features, &modules);

        assert_eq!(cov.surfaces_total, 3);
        assert_eq!(
            cov.surfaces_documented, 2,
            "only non-empty summaries count as documented"
        );
        assert!(cov.modules_orphaned.is_empty());
    }

    #[test]
    fn compute_coverage_orphaned_list_is_sorted() {
        let features: Vec<Feature> = vec![];
        let modules = vec![
            Module {
                path: "src/zoo".into(),
                ..Module::default()
            },
            Module {
                path: "src/alpha".into(),
                ..Module::default()
            },
            Module {
                path: "src/beta".into(),
                ..Module::default()
            },
        ];

        let cov = compute_coverage(&features, &modules);

        assert_eq!(
            cov.modules_orphaned,
            vec!["src/alpha", "src/beta", "src/zoo"],
            "orphaned list must be sorted"
        );
    }
}
