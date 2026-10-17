use super::*;
use crate::session::types::Session;
use std::time::{Duration, UNIX_EPOCH};

fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_000_000)
}

fn transcript(id: &str, cwd: &str, title: &str, age_secs: u64) -> DiscoveredSession {
    DiscoveredSession {
        session_id: id.to_string(),
        cwd: PathBuf::from(cwd),
        title: title.to_string(),
        last_active: now() - Duration::from_secs(age_secs),
        skipped_lines: 0,
    }
}

fn pool_session(title: &str) -> Session {
    let mut s = Session::new(
        String::new(),
        "opus".into(),
        "orchestrator".into(),
        None,
        None,
    );
    s.issue_title = Some(title.to_string());
    s
}

fn row(title: &str, repo: &str, age_secs: Option<u64>) -> FleetRow {
    FleetRow {
        kind: FleetKind::External,
        source: FleetSource::Transcript {
            agent_session_id: title.to_string(),
        },
        repo: repo.to_string(),
        title: title.to_string(),
        status: "external".to_string(),
        cwd: None,
        last_active: age_secs.map(|s| now() - Duration::from_secs(s)),
    }
}

fn titles(rows: &[FleetRow], indices: &[usize]) -> Vec<String> {
    indices.iter().map(|&i| rows[i].title.clone()).collect()
}

#[test]
fn build_rows_lists_unbound_pool_session_as_managed() {
    let session = pool_session("queued work");
    let rows = build_rows(&[&session], "maestro", &[]);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, FleetKind::Managed);
    assert_eq!(rows[0].repo, "maestro");
    assert_eq!(rows[0].title, "queued work");
    assert_eq!(rows[0].last_active, None, "never started: no activity time");
}

#[test]
fn build_rows_lists_unadopted_transcript_as_external_with_repo_from_cwd() {
    let rows = build_rows(
        &[],
        "maestro",
        &[transcript("ext-1", "/home/dev/api-service", "add auth", 60)],
    );

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, FleetKind::External);
    assert_eq!(rows[0].repo, "api-service");
    assert_eq!(rows[0].title, "add auth");
    assert_eq!(
        rows[0].source,
        FleetSource::Transcript {
            agent_session_id: "ext-1".into()
        }
    );
    assert_eq!(rows[0].cwd, Some(PathBuf::from("/home/dev/api-service")));
}

#[test]
fn adopted_transcript_is_listed_once_as_adopted_pool_row() {
    let mut adopted = pool_session("fix parser");
    assert!(adopted.bind_agent_session_id("ext-1"));
    adopted.adopted_cwd = Some(PathBuf::from("/home/dev/api-service"));

    let rows = build_rows(
        &[&adopted],
        "maestro",
        &[transcript(
            "ext-1",
            "/home/dev/api-service",
            "fix parser",
            60,
        )],
    );

    assert_eq!(rows.len(), 1, "transcript must not appear a second time");
    assert_eq!(rows[0].kind, FleetKind::Adopted);
    assert_eq!(rows[0].repo, "api-service");
    assert!(matches!(rows[0].source, FleetSource::Pool { .. }));
}

#[test]
fn activity_desc_puts_most_recent_first_and_never_started_last() {
    let rows = vec![
        row("older", "a", Some(300)),
        row("never", "a", None),
        row("newest", "a", Some(10)),
        row("middle", "a", Some(100)),
    ];

    let order = sorted_indices(&rows, "", FleetSort::ActivityDesc);

    assert_eq!(
        titles(&rows, &order),
        ["newest", "middle", "older", "never"]
    );
}

#[test]
fn stale_first_puts_least_recently_active_first_and_never_started_first() {
    let rows = vec![
        row("older", "a", Some(300)),
        row("never", "a", None),
        row("newest", "a", Some(10)),
        row("middle", "a", Some(100)),
    ];

    let order = sorted_indices(&rows, "", FleetSort::StaleFirst);

    assert_eq!(
        titles(&rows, &order),
        ["never", "older", "middle", "newest"]
    );
}

#[test]
fn sort_toggle_flips_between_the_two_orders() {
    assert_eq!(FleetSort::default(), FleetSort::ActivityDesc);
    assert_eq!(FleetSort::ActivityDesc.toggled(), FleetSort::StaleFirst);
    assert_eq!(FleetSort::StaleFirst.toggled(), FleetSort::ActivityDesc);
}

#[test]
fn filter_matches_repo_and_title_case_insensitively() {
    let rows = vec![
        row("add auth endpoint", "api-service", Some(10)),
        row("docs pass", "maestro", Some(20)),
    ];

    let by_repo = sorted_indices(&rows, "API", FleetSort::ActivityDesc);
    assert_eq!(titles(&rows, &by_repo), ["add auth endpoint"]);

    let by_title = sorted_indices(&rows, "docs", FleetSort::ActivityDesc);
    assert_eq!(titles(&rows, &by_title), ["docs pass"]);

    assert!(sorted_indices(&rows, "zzz-none", FleetSort::ActivityDesc).is_empty());
}
