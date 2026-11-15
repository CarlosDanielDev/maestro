use super::*;

#[test]
fn parse_none_fixture() {
    let body = include_str!("../../tests/fixtures/blocked_by/none.md");
    assert!(parse_blocked_by(body).is_empty());
}

#[test]
fn parse_single_fixture() {
    let body = include_str!("../../tests/fixtures/blocked_by/single.md");
    assert_eq!(parse_blocked_by(body), vec![123]);
}

#[test]
fn parse_multi_fixture() {
    let body = include_str!("../../tests/fixtures/blocked_by/multi.md");
    let mut deps = parse_blocked_by(body);
    deps.sort_unstable();
    assert_eq!(deps, vec![10, 20, 30]);
}

#[test]
fn parse_malformed_fixture() {
    let body = include_str!("../../tests/fixtures/blocked_by/malformed.md");
    assert_eq!(parse_blocked_by(body), vec![77]);
}

#[test]
fn parse_missing_fixture() {
    let body = include_str!("../../tests/fixtures/blocked_by/missing.md");
    assert!(parse_blocked_by(body).is_empty());
}

fn meta(
    n: IssueNumber,
    state: IssueState,
    milestone: Option<u64>,
    blocked_by: Vec<IssueNumber>,
) -> IssueMeta {
    IssueMeta {
        number: n,
        state,
        milestone,
        blocked_by,
    }
}

#[test]
fn classify_in_slice() {
    let mut metas = HashMap::new();
    metas.insert(1, meta(1, IssueState::Open, Some(1), vec![2]));
    metas.insert(2, meta(2, IssueState::Open, Some(1), vec![]));
    let selected = HashSet::from([1u64, 2]);
    let edges = classify_edges(&selected, Some(1), &metas);
    assert_eq!(edges.get(&1), Some(&vec![Edge::InSlice(2)]));
}

#[test]
fn classify_closed_external() {
    let mut metas = HashMap::new();
    metas.insert(1, meta(1, IssueState::Open, Some(1), vec![3]));
    metas.insert(3, meta(3, IssueState::Closed, Some(1), vec![]));
    let selected = HashSet::from([1u64]);
    let edges = classify_edges(&selected, Some(1), &metas);
    assert_eq!(edges.get(&1), Some(&vec![Edge::ClosedExternal(3)]));
}

#[test]
fn classify_same_milestone_open_external() {
    let mut metas = HashMap::new();
    metas.insert(1, meta(1, IssueState::Open, Some(1), vec![4]));
    metas.insert(4, meta(4, IssueState::Open, Some(1), vec![]));
    let selected = HashSet::from([1u64]);
    let edges = classify_edges(&selected, Some(1), &metas);
    assert_eq!(
        edges.get(&1),
        Some(&vec![Edge::SameMilestoneOpenExternal(4)])
    );
}

#[test]
fn classify_cross_milestone_open_external_when_missing_meta() {
    let mut metas = HashMap::new();
    metas.insert(1, meta(1, IssueState::Open, Some(1), vec![9]));
    let selected = HashSet::from([1u64]);
    let edges = classify_edges(&selected, Some(1), &metas);
    assert_eq!(
        edges.get(&1),
        Some(&vec![Edge::CrossMilestoneOpenExternal(9)])
    );
}

#[test]
fn topo_linear_chain() {
    let selected = HashSet::from([1u64, 2, 3]);
    let mut edges = HashMap::new();
    edges.insert(2, vec![Edge::InSlice(1)]);
    edges.insert(3, vec![Edge::InSlice(2)]);
    let levels = topo_levels(&selected, &edges).unwrap();
    assert_eq!(levels, vec![vec![1], vec![2], vec![3]]);
}

#[test]
fn topo_parallel_leaves() {
    let selected = HashSet::from([1u64, 2, 3]);
    let levels = topo_levels(&selected, &HashMap::new()).unwrap();
    assert_eq!(levels.len(), 1);
    let mut l0 = levels[0].clone();
    l0.sort_unstable();
    assert_eq!(l0, vec![1, 2, 3]);
}

#[test]
fn topo_cycle_reports_path() {
    let selected = HashSet::from([1u64, 2]);
    let mut edges = HashMap::new();
    edges.insert(1, vec![Edge::InSlice(2)]);
    edges.insert(2, vec![Edge::InSlice(1)]);
    let err = topo_levels(&selected, &edges).unwrap_err();
    assert!(err.to_string().contains("cycle"));
}

#[test]
fn auto_expand_adds_same_milestone() {
    let selected = HashSet::from([1u64, 2]);
    let mut edges = HashMap::new();
    edges.insert(2, vec![Edge::SameMilestoneOpenExternal(3)]);
    let r = auto_expand(selected.clone(), &edges);
    match r {
        ExpandResult::Expanded { selected: s, added } => {
            assert!(s.contains(&3));
            assert_eq!(added, vec![3]);
        }
        _ => panic!("expected Expanded"),
    }
}

#[test]
fn auto_expand_refuses_when_more_than_double() {
    let selected = HashSet::from([1u64]);
    let mut edges = HashMap::new();
    edges.insert(
        1,
        vec![
            Edge::SameMilestoneOpenExternal(2),
            Edge::SameMilestoneOpenExternal(3),
            Edge::SameMilestoneOpenExternal(4),
        ],
    );
    let r = auto_expand(selected, &edges);
    assert!(matches!(r, ExpandResult::TooLarge { .. }));
}
