use super::*;
use crate::orchestration::dag::{IssueState, parse_blocked_by};
use crate::work::conflicts::files_to_modify_from_body;

const MS: Option<u64> = Some(1);

fn meta(number: u64, state: IssueState, milestone: Option<u64>, blocked_by: &[u64]) -> IssueMeta {
    IssueMeta {
        number,
        state,
        milestone,
        blocked_by: blocked_by.to_vec(),
    }
}

fn open_meta(number: u64, blocked_by: &[u64]) -> IssueMeta {
    meta(number, IssueState::Open, MS, blocked_by)
}

fn metas_of(list: Vec<IssueMeta>) -> HashMap<u64, IssueMeta> {
    list.into_iter().map(|m| (m.number, m)).collect()
}

fn set(ns: &[u64]) -> HashSet<u64> {
    ns.iter().copied().collect()
}

fn plan(open: &[u64], metas: &HashMap<u64, IssueMeta>) -> RunPlan {
    run_plan(&set(open), metas, &HashSet::new(), &HashMap::new(), 1.0).unwrap()
}

#[test]
fn run_plan_levels() {
    struct Case {
        name: &'static str,
        open: &'static [u64],
        metas: Vec<IssueMeta>,
        waves: Vec<Vec<u64>>,
    }
    let cases = vec![
        Case {
            name: "no edges: one wave",
            open: &[3, 1, 2],
            metas: vec![open_meta(1, &[]), open_meta(2, &[]), open_meta(3, &[])],
            waves: vec![vec![1, 2, 3]],
        },
        Case {
            name: "chain",
            open: &[1, 2, 3],
            metas: vec![open_meta(1, &[]), open_meta(2, &[1]), open_meta(3, &[2])],
            waves: vec![vec![1], vec![2], vec![3]],
        },
        Case {
            name: "diamond",
            open: &[1, 2, 3, 4],
            metas: vec![
                open_meta(1, &[]),
                open_meta(2, &[1]),
                open_meta(3, &[1]),
                open_meta(4, &[2, 3]),
            ],
            waves: vec![vec![1], vec![2, 3], vec![4]],
        },
        Case {
            name: "closed blocker is satisfied",
            open: &[2, 3],
            metas: vec![
                meta(99, IssueState::Closed, MS, &[]),
                meta(98, IssueState::Closed, Some(7), &[]),
                open_meta(2, &[99]),
                open_meta(3, &[2, 98]),
            ],
            waves: vec![vec![2], vec![3]],
        },
    ];
    for c in cases {
        let metas = metas_of(c.metas);
        let p = plan(c.open, &metas);
        assert_eq!(p.waves, c.waves, "{}", c.name);
        assert!(p.deferred.is_empty(), "{}: {:?}", c.name, p.deferred);
        assert_eq!(p, plan(c.open, &metas), "{}: not deterministic", c.name);
    }
}

#[test]
fn run_plan_cycle_is_an_error() {
    let metas = metas_of(vec![open_meta(1, &[2]), open_meta(2, &[1])]);
    let r = run_plan(&set(&[1, 2]), &metas, &HashSet::new(), &HashMap::new(), 1.0);
    assert!(r.is_err());
}

fn body(blocked_by: &str, files: &[&str]) -> String {
    let list: String = files.iter().map(|f| format!("- `{f}` — edit\n")).collect();
    format!("## Overview\n\nx\n\n## Files to Modify\n\n{list}\n## Blocked By\n\n- {blocked_by}\n")
}

#[test]
fn run_plan_splits_file_overlap() {
    // Realistic shape: blocked-by and file lists both parsed from issue bodies.
    let bodies: Vec<(u64, String)> = vec![
        (10, body("None", &["crates/a/src/ui.rs"])),
        (
            11,
            body("None", &["crates/a/src/x.rs", "crates/a/src/ui.rs"]),
        ),
        (12, body("None", &["crates/a/src/other.rs"])),
        (13, body("#11", &["crates/a/src/y.rs"])),
        (14, body("None", &[])),
    ];
    let metas = metas_of(
        bodies
            .iter()
            .map(|(n, b)| open_meta(*n, &parse_blocked_by(b)))
            .collect(),
    );
    let files: HashMap<u64, Option<Vec<String>>> = bodies
        .iter()
        .map(|(n, b)| (*n, files_to_modify_from_body(b)))
        .collect();
    let open = set(&[10, 11, 12, 13, 14]);

    let p = run_plan(&open, &metas, &HashSet::new(), &files, 1.0).unwrap();

    assert_eq!(p.waves, vec![vec![10, 12, 14], vec![11], vec![13]]);
    assert_eq!(p.moved, vec![(11, 10, "crates/a/src/ui.rs".to_string())]);
    assert_eq!(p.unknown_scope, vec![14]);

    // Invariants: no shared file inside a wave; every blocker in an earlier wave.
    let wave_of: HashMap<u64, usize> = p
        .waves
        .iter()
        .enumerate()
        .flat_map(|(i, w)| w.iter().map(move |n| (*n, i)))
        .collect();
    for w in &p.waves {
        let mut seen = HashSet::new();
        for n in w {
            for f in files[n].iter().flatten() {
                assert!(seen.insert(f.clone()), "{f} shared inside wave {w:?}");
            }
        }
    }
    for (n, m) in &metas {
        for d in &m.blocked_by {
            assert!(
                wave_of[d] < wave_of[n],
                "#{n} shares or precedes its blocker #{d}"
            );
        }
    }
}

#[test]
fn run_plan_deferred_reasons() {
    let metas = metas_of(vec![
        open_meta(4, &[]),
        open_meta(5, &[]),
        open_meta(6, &[200]),
        open_meta(7, &[6]),
        open_meta(8, &[300]),
        open_meta(9, &[5]),
        meta(200, IssueState::Open, Some(9), &[]),
    ]);
    let open = set(&[4, 5, 6, 7, 8, 9]);
    let claimed = set(&[5, 77]);

    let p = run_plan(&open, &metas, &claimed, &HashMap::new(), 1.0).unwrap();

    assert_eq!(p.waves, vec![vec![4]]);
    assert_eq!(
        p.deferred,
        vec![
            (5, Hold::Claimed),
            (
                6,
                Hold::BlockedOutside {
                    by: 200,
                    milestone: Some(9)
                }
            ),
            (7, Hold::WaitsOnDeferred(6)),
            (
                8,
                Hold::BlockedOutside {
                    by: 300,
                    milestone: None
                }
            ),
            (9, Hold::WaitsOnDeferred(5)),
        ]
    );
    // 77 is claimed but not in the plan: nothing to decide.
    assert_eq!(p.needs_decision, vec![5]);
}

#[test]
fn run_plan_worst_case() {
    let metas = metas_of(vec![
        open_meta(1, &[]),
        open_meta(2, &[1]),
        open_meta(3, &[]),
        open_meta(4, &[500]),
    ]);
    let open = set(&[1, 2, 3, 4]);
    let none = HashSet::new();
    let files = HashMap::new();

    let p = run_plan(&open, &metas, &none, &files, 2.5).unwrap();
    // 3 issues in waves (4 is deferred) x $2.50.
    assert_eq!(p.worst_case_usd, 7.5);

    let p = run_plan(&open, &metas, &none, &files, 0.0).unwrap();
    assert_eq!(p.worst_case_usd, 0.0);

    let p = run_plan(&HashSet::new(), &metas, &none, &files, 2.5).unwrap();
    assert_eq!(p.worst_case_usd, 0.0);
}
