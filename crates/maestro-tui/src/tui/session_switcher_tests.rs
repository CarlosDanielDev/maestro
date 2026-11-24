use super::*;
use crate::session::types::Session;

fn make_session(prompt: &str, issue: Option<u64>) -> Session {
    Session::new(
        prompt.into(),
        "opus".into(),
        "orchestrator".into(),
        issue,
        None,
    )
}

#[test]
fn default_has_no_selection_and_empty_filter() {
    let s = SessionSwitcher::default();
    assert_eq!(s.selected, None);
    assert!(s.filter_text.is_empty());
}

#[test]
fn filtered_sessions_with_empty_filter_returns_all() {
    let s1 = make_session("fix auth", None);
    let s2 = make_session("add dashboard", None);
    let s3 = make_session("fix login", None);
    let sessions = vec![&s1, &s2, &s3];
    let sw = SessionSwitcher::default();
    let result = sw.filtered_sessions(&sessions);
    assert_eq!(result.len(), 3);
}

#[test]
fn filtered_sessions_matches_prompt_substring() {
    let s1 = make_session("fix auth bug", None);
    let s2 = make_session("add dashboard", None);
    let s3 = make_session("fix login", None);
    let sessions = vec![&s1, &s2, &s3];
    let sw = SessionSwitcher {
        selected: None,
        filter_text: "fix".into(),
    };
    let result = sw.filtered_sessions(&sessions);
    assert_eq!(result.len(), 2);
}

#[test]
fn filtered_sessions_matches_issue_number() {
    let s1 = make_session("work on auth", Some(42));
    let s2 = make_session("dashboard", Some(99));
    let s3 = make_session("no issue", None);
    let sessions = vec![&s1, &s2, &s3];
    let sw = SessionSwitcher {
        selected: None,
        filter_text: "42".into(),
    };
    let result = sw.filtered_sessions(&sessions);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].issue_number, Some(42));
}

#[test]
fn filtered_sessions_case_insensitive() {
    let s = make_session("Fix Auth Bug", None);
    let sessions = vec![&s];
    let sw = SessionSwitcher {
        selected: None,
        filter_text: "fix".into(),
    };
    assert_eq!(sw.filtered_sessions(&sessions).len(), 1);
}

#[test]
fn filtered_sessions_empty_pool_returns_empty() {
    let sessions: Vec<&Session> = vec![];
    let sw = SessionSwitcher::default();
    assert!(sw.filtered_sessions(&sessions).is_empty());
}

#[test]
fn filtered_sessions_no_match_returns_empty() {
    let s1 = make_session("fix auth", None);
    let s2 = make_session("add dashboard", None);
    let sessions = vec![&s1, &s2];
    let sw = SessionSwitcher {
        selected: None,
        filter_text: "zzznomatch".into(),
    };
    assert!(sw.filtered_sessions(&sessions).is_empty());
}

#[test]
fn selected_session_filtered_out_is_none() {
    let s1 = make_session("fix auth", None);
    let s2 = make_session("docs", None);
    let sessions = vec![&s1, &s2];
    let sw = SessionSwitcher {
        selected: Some(s2.id),
        filter_text: "fix".into(),
    };
    assert!(sw.selected_session(&sessions).is_none());
}

#[test]
fn selected_session_none_returns_first() {
    let s1 = make_session("fix auth", None);
    let s2 = make_session("fix login", None);
    let s3 = make_session("fix ui", None);
    let sessions = vec![&s1, &s2, &s3];
    let sw = SessionSwitcher {
        selected: None,
        filter_text: "fix".into(),
    };
    assert_eq!(sw.selected_session(&sessions).unwrap().id, s1.id);
}

#[test]
fn move_up_at_first_stays_at_first() {
    let s1 = make_session("a", None);
    let s2 = make_session("b", None);
    let sessions = vec![&s1, &s2];
    let mut sw = SessionSwitcher::default();
    sw.move_up(&sessions);
    assert_eq!(sw.selected, Some(s1.id));
}

#[test]
fn move_down_steps_and_stops_at_last() {
    let s1 = make_session("a", None);
    let s2 = make_session("b", None);
    let sessions = vec![&s1, &s2];
    let mut sw = SessionSwitcher::default();
    sw.move_down(&sessions);
    assert_eq!(sw.selected, Some(s2.id));
    sw.move_down(&sessions);
    assert_eq!(sw.selected, Some(s2.id));
}

#[test]
fn most_urgent_sessions_rank_first() {
    let mut done = make_session("done", None);
    done.status = SessionStatus::Completed;
    let running = make_session("running", None);
    let mut failed = make_session("failed", None);
    failed.status = SessionStatus::FailedGates;
    let sessions = vec![&done, &running, &failed];
    let order: Vec<_> = SessionSwitcher::default()
        .filtered_sessions(&sessions)
        .iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(order, vec![failed.id, running.id, done.id]);
}

#[test]
fn active_status_symbol_uses_braille_spinner_when_nerd_font_enabled() {
    assert_eq!(
        session_switcher_status_symbol(SessionStatus::Running, 3, true),
        "⠸"
    );
    assert_eq!(
        session_switcher_status_symbol(SessionStatus::Spawning, 0, true),
        "⠋"
    );
}

#[test]
fn active_status_symbol_uses_ascii_spinner_without_nerd_font() {
    assert_eq!(
        session_switcher_status_symbol(SessionStatus::Running, 1, false),
        "/"
    );
}

#[test]
fn terminal_status_symbol_keeps_status_icon() {
    assert_eq!(
        session_switcher_status_symbol(SessionStatus::Completed, 3, true),
        SessionStatus::Completed.symbol().to_string()
    );
}
