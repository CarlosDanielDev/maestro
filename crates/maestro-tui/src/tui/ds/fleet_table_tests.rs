use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use super::*;

fn row(status: SessionStatus, issue: Option<u64>, title: &str) -> FleetRow {
    FleetRow {
        status,
        issue,
        title: title.to_string(),
        model: "claude-opus-4-5".to_string(),
        context: 0.35,
        cost: Some(1.25),
        elapsed: "12m03s".to_string(),
    }
}

fn busy_rows() -> Vec<FleetRow> {
    vec![
        row(SessionStatus::Running, Some(409), "Token refresh"),
        FleetRow {
            context: 0.83,
            ..row(SessionStatus::FailedGates, Some(412), "Auth refactor")
        },
        FleetRow {
            cost: None,
            ..row(
                SessionStatus::NeedsPr,
                Some(405),
                "Docs for the fleet board",
            )
        },
        FleetRow {
            elapsed: "—".to_string(),
            ..row(SessionStatus::Queued, None, "Run the prompt")
        },
    ]
}

fn render(rows: &[FleetRow], w: u16, h: u16, compact: bool) -> String {
    let theme = Theme::dark();
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| draw(f, f.area(), rows, compact, &theme, true))
        .unwrap();
    let buf = t.backend().buffer().clone();
    (0..h)
        .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn fleet_table_regular_shows_seven_columns() {
    let out = render(&busy_rows(), 110, 7, false);
    for header in [
        "STATUS", "ISSUE", "TITLE", "MODEL", "CONTEXT", "COST", "TIME",
    ] {
        assert!(out.contains(header), "missing {header}:\n{out}");
    }
    assert!(out.contains("Fleet (4)"), "{out}");
    assert!(out.contains("$?"), "unpriced cost reads $?:\n{out}");
    assert_snapshot!("fleet_table_regular", out);
}

#[test]
fn fleet_table_compact_keeps_five_columns() {
    let out = render(&busy_rows(), 80, 7, true);
    for header in ["STATUS", "ISSUE", "TITLE", "CTX", "COST"] {
        assert!(out.contains(header), "missing {header}:\n{out}");
    }
    for gone in ["MODEL", "CONTEXT", "TIME"] {
        assert!(!out.contains(gone), "{gone} must drop on compact:\n{out}");
    }
    assert_snapshot!("fleet_table_compact", out);
}

#[test]
fn fleet_table_empty_teaches_the_next_step() {
    let out = render(&[], 80, 4, false);
    assert!(out.contains("No agents yet"), "{out}");
    assert!(out.contains("i new issue"), "{out}");
}
