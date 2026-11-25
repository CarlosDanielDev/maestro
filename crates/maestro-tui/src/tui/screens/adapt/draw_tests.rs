//! #1281: the Configure step shows the live "This run will" plan, and the
//! Review step shows what will be written behind the armed ⏎ ⏎.

use super::*;
use crate::adapt::types::{AdaptPlan, PlannedIssue, PlannedMilestone};
use crate::tui::theme::Theme;
use ratatui::{Terminal, backend::TestBackend};

/// Wide breakpoint (§02.11 measures 168×42).
const WIDE: (u16, u16) = (168, 42);

fn clean_screen() -> AdaptScreen {
    let mut s = AdaptScreen::with_provider_kind(ProviderKind::Github);
    // `with_provider_kind` may load a stale cache from the CWD; the
    // snapshot shows only what this fixture sets.
    s.results = AdaptResults::default();
    s.loaded_from_cache = false;
    s
}

fn draw(s: &mut AdaptScreen) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(WIDE.0, WIDE.1)).unwrap();
    let theme = Theme::dark();
    t.draw(|f| s.draw(f, f.area(), &theme)).unwrap();
    t
}

fn text(t: &Terminal<TestBackend>) -> String {
    let buf = t.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn adapt_configure_wide() {
    let mut s = clean_screen();
    let t = draw(&mut s);
    let shown = text(&t);
    assert!(shown.contains("This run will"), "the live plan is drawn");
    assert!(
        shown.contains("Plan · dry run"),
        "the default option is named"
    );
    insta::assert_snapshot!(t.backend());
}

#[test]
fn configure_plan_follows_the_radio() {
    let mut s = clean_screen();
    let dry = text(&draw(&mut s));
    StopAfter::Full.apply(&mut s.config);
    let full = text(&draw(&mut s));
    assert!(dry.contains("CREATE ON REMOTE") && dry.contains("nothing"));
    assert!(
        full.contains("labels, milestones, issues"),
        "Full run lists the GitHub writes"
    );
}

#[test]
fn adapt_review_wide() {
    let mut s = clean_screen();
    StopAfter::Full.apply(&mut s.config);
    s.step = AdaptStep::Review;
    s.results.plan = Some(AdaptPlan {
        milestones: vec![PlannedMilestone {
            title: "M0: Foundation".into(),
            description: "d".into(),
            issues: vec![
                PlannedIssue {
                    title: "feat: setup".into(),
                    body: "b".into(),
                    labels: vec!["enhancement".into()],
                    blocked_by_titles: vec![],
                },
                PlannedIssue {
                    title: "test: add tests".into(),
                    body: "b".into(),
                    labels: vec!["testing".into()],
                    blocked_by_titles: vec!["feat: setup".into()],
                },
            ],
        }],
        maestro_toml_patch: None,
        workflow_guide: None,
    });
    let t = draw(&mut s);
    let shown = text(&t);
    assert!(shown.contains("nothing written yet"), "the writes ledger");
    assert!(shown.contains("feat: setup"), "the plan tree");
    insta::assert_snapshot!(t.backend());
}
