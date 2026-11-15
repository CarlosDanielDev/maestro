use crate::budget::BudgetEnforcer;
use crate::tui::budget_banner::draw_budget_banner_if_alerting;
use crate::tui::theme::Theme;
use insta::assert_snapshot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

#[test]
fn budget_banner_renders_at_80pct() {
    let mut terminal = Terminal::new(TestBackend::new(80, 3)).unwrap();
    let theme = Theme::dark();
    let enforcer = BudgetEnforcer::new(5.0, 50.0, 80);

    terminal
        .draw(|f| {
            draw_budget_banner_if_alerting(f, Some(&enforcer), 40.0, f.area(), &theme);
        })
        .unwrap();

    let output = format!("{}", terminal.backend());
    assert!(
        output.contains("80%") || output.contains("BUDGET ALERT"),
        "banner must mention 80% threshold; output:\n{}",
        output
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn budget_banner_renders_at_90pct() {
    let mut terminal = Terminal::new(TestBackend::new(80, 3)).unwrap();
    let theme = Theme::dark();
    let enforcer = BudgetEnforcer::new(5.0, 50.0, 80);

    terminal
        .draw(|f| {
            draw_budget_banner_if_alerting(f, Some(&enforcer), 45.0, f.area(), &theme);
        })
        .unwrap();

    let output = format!("{}", terminal.backend());
    assert!(
        output.contains("90%"),
        "banner must show 90% at this cost; output:\n{}",
        output
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn budget_banner_renders_at_100pct() {
    let mut terminal = Terminal::new(TestBackend::new(80, 3)).unwrap();
    let theme = Theme::dark();
    let enforcer = BudgetEnforcer::new(5.0, 50.0, 80);

    terminal
        .draw(|f| {
            draw_budget_banner_if_alerting(f, Some(&enforcer), 50.0, f.area(), &theme);
        })
        .unwrap();

    let output = format!("{}", terminal.backend());
    assert!(
        output.contains("BUDGET EXCEEDED") || output.contains("$50.00"),
        "banner must mark Kill state at limit; output:\n{}",
        output
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn budget_banner_absent_when_under_threshold() {
    let mut terminal = Terminal::new(TestBackend::new(80, 3)).unwrap();
    let theme = Theme::dark();
    let enforcer = BudgetEnforcer::new(5.0, 50.0, 80);
    let mut drawn = false;

    terminal
        .draw(|f| {
            drawn = draw_budget_banner_if_alerting(f, Some(&enforcer), 10.0, f.area(), &theme);
        })
        .unwrap();

    assert!(!drawn, "banner must be a no-op under threshold");
    let output = format!("{}", terminal.backend());
    let body: String = output
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '"' && *c != '\n')
        .collect();
    assert!(
        body.is_empty(),
        "rendered output must be blank when below threshold; got:\n{}",
        output
    );
}

#[test]
fn budget_banner_absent_when_enforcer_is_none() {
    let mut terminal = Terminal::new(TestBackend::new(80, 3)).unwrap();
    let theme = Theme::dark();
    let mut drawn = false;

    terminal
        .draw(|f| {
            drawn = draw_budget_banner_if_alerting(f, None, 99.0, f.area(), &theme);
        })
        .unwrap();

    assert!(!drawn, "banner must be a no-op when enforcer is None");
    let output = format!("{}", terminal.backend());
    let body: String = output
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '"' && *c != '\n')
        .collect();
    assert!(
        body.is_empty(),
        "rendered output must be blank when enforcer is None; got:\n{}",
        output
    );
}
