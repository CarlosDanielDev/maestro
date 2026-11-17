use chrono::{Duration, TimeZone};
use insta::assert_snapshot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 17, 15, 0, 0).unwrap()
}

fn session(n: u128, status: SessionStatus, since_min: Option<i64>) -> SessionSignal {
    SessionSignal {
        id: uuid::Uuid::from_u128(n),
        issue: Some(400 + n as u64),
        title: None,
        status,
        since: since_min.map(|m| t0() + Duration::minutes(m)),
    }
}

/// Short tag per row so the table reads as one line per expectation.
fn tag(item: &AttentionItem) -> String {
    match &item.subject {
        Subject::Session { issue, .. } => format!("#{}", issue.unwrap_or(0)),
        Subject::DoctorCheck => "doctor".into(),
        Subject::GhAuth => "gh-auth".into(),
        Subject::Budget => "budget".into(),
        Subject::Quota { provider } => format!("quota:{provider}"),
    }
}

fn mixed_input() -> AttentionInput {
    AttentionInput {
        sessions: vec![
            session(1, SessionStatus::NeedsPr, Some(1)),
            session(2, SessionStatus::NeedsReview, Some(5)),
            session(3, SessionStatus::Running, Some(0)),
            session(4, SessionStatus::FailedGates, Some(10)),
            session(5, SessionStatus::NeedsReview, Some(2)),
            session(6, SessionStatus::Errored, None),
            session(7, SessionStatus::Completed, Some(0)),
            session(8, SessionStatus::NeedsPr, Some(1)),
        ],
        doctor_failures: vec!["git: not a git repository".into()],
        gh_auth_lost: true,
        budget: Some(BudgetSignal {
            spent: 16.0,
            cap: 20.0,
        }),
        quota: vec![QuotaSignal {
            provider: "claude".into(),
            used_pct: 92.0,
        }],
    }
}

#[test]
fn attention_rank_order() {
    let ranked = rank(&mixed_input());
    let got: Vec<(AttentionKind, String)> = ranked.iter().map(|i| (i.kind, tag(i))).collect();
    let want = [
        // Errors: dated first (oldest wins), then undated in input order.
        (AttentionKind::Error, "#404"),
        (AttentionKind::Error, "#406"),
        (AttentionKind::Error, "doctor"),
        (AttentionKind::Error, "gh-auth"),
        // Waiting on you: oldest first.
        (AttentionKind::NeedsReview, "#405"),
        (AttentionKind::NeedsReview, "#402"),
        // Ready to ship: same age, so input order stays.
        (AttentionKind::NeedsPr, "#401"),
        (AttentionKind::NeedsPr, "#408"),
        // Resources last.
        (AttentionKind::Resource, "budget"),
        (AttentionKind::Resource, "quota:claude"),
    ];
    let want: Vec<(AttentionKind, String)> =
        want.iter().map(|(k, t)| (*k, (*t).to_string())).collect();
    assert_eq!(got, want);
}

#[test]
fn rank_skips_sessions_that_do_not_need_you() {
    let input = AttentionInput {
        sessions: [
            SessionStatus::Queued,
            SessionStatus::Running,
            SessionStatus::GatesRunning,
            SessionStatus::CiFix,
            SessionStatus::Completed,
            SessionStatus::Killed,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, s)| session(i as u128, s, None))
        .collect(),
        ..AttentionInput::default()
    };
    assert!(rank(&input).is_empty());
}

#[test]
fn lost_gh_auth_is_one_row_not_two() {
    let input = AttentionInput {
        doctor_failures: vec!["gh auth: not logged in".into(), "git: missing".into()],
        gh_auth_lost: true,
        ..AttentionInput::default()
    };
    let tags: Vec<String> = rank(&input).iter().map(tag).collect();
    assert_eq!(tags, ["doctor", "gh-auth"]);
}

#[test]
fn reasons_are_sanitized() {
    let mut s = session(1, SessionStatus::NeedsReview, None);
    s.title = Some("evil\u{1b}[2Jtitle".into());
    let input = AttentionInput {
        sessions: vec![s],
        doctor_failures: vec!["git:\u{7} bell".into()],
        ..AttentionInput::default()
    };
    for item in rank(&input) {
        assert!(
            !item.reason.chars().any(char::is_control),
            "control char in {:?}",
            item.reason
        );
    }
}

#[test]
fn title_is_plain_when_nothing_needs_you() {
    assert_eq!(terminal_title(0), "maestro");
    assert_eq!(terminal_title(2), "(2) maestro");
}

#[test]
fn exit_title_has_no_count() {
    assert_eq!(exit_title(), "maestro");
    assert_eq!(exit_title(), terminal_title(0));
}

fn draw_queue(w: u16, items: &[AttentionItem], fleet_busy: bool) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(w, 24)).unwrap();
    let h = items.len().max(3) as u16 + 2;
    let view = QueueView {
        focused: true,
        selected: 0,
        fleet_busy,
        now: t0() + Duration::minutes(20),
    };
    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, w, h);
            draw(f, area, items, view, &Theme::dark(), true);
        })
        .unwrap();
    terminal
}

fn queue_fixture() -> Vec<AttentionItem> {
    let mut gates = session(12, SessionStatus::FailedGates, Some(18));
    gates.title = Some("auth-refactor".into());
    let mut review = session(2, SessionStatus::NeedsReview, Some(15));
    review.issue = Some(398);
    review.title = Some("tui-palette".into());
    let mut pr = session(5, SessionStatus::NeedsPr, Some(8));
    pr.title = Some("docs-sync".into());
    rank(&AttentionInput {
        sessions: vec![pr, review, gates],
        budget: Some(BudgetSignal {
            spent: 16.0,
            cap: 20.0,
        }),
        quota: vec![QuotaSignal {
            provider: "claude".into(),
            used_pct: 92.0,
        }],
        ..AttentionInput::default()
    })
}

#[test]
fn attention_80() {
    assert_snapshot!(draw_queue(80, &queue_fixture(), true).backend());
}

#[test]
fn attention_150() {
    assert_snapshot!(draw_queue(150, &queue_fixture(), true).backend());
}

#[test]
fn attention_empty_idle() {
    let terminal = draw_queue(80, &[], false);
    let text = terminal.backend().to_string();
    assert!(text.contains("Nothing needs you. Start something: i f a"));
    assert_snapshot!(terminal.backend());
}

#[test]
fn attention_empty_busy() {
    let terminal = draw_queue(80, &[], true);
    let text = terminal.backend().to_string();
    assert!(text.contains("All clear. Agents are working."));
    assert!(!text.contains("Nothing needs you"));
    assert_snapshot!(terminal.backend());
}
