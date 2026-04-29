use insta::assert_snapshot;

use crate::config::Config;
use crate::session::types::{Session, SessionStatus};
use crate::tui::app::TuiMode;
use crate::tui::snapshot_tests::{fixed_end, fixed_start, test_terminal};
use crate::tui::{make_test_app, ui};

fn make_config(agent_graph_enabled: bool) -> Config {
    let toml = format!(
        "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\n\
         per_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
         [github]\n[notifications]\n[views]\nagent_graph_enabled = {agent_graph_enabled}\n"
    );
    toml::from_str(&toml).expect("test config parse")
}

fn two_agent_sessions() -> (Session, Session) {
    use uuid::Uuid;

    let mut s1 = Session::new(
        "Implement login".to_string(),
        "claude-opus-4-5".to_string(),
        "orchestrator".to_string(),
        Some(101),
    );
    s1.id = Uuid::nil();
    s1.status = SessionStatus::Running;
    s1.started_at = Some(fixed_start());
    s1.finished_at = Some(fixed_end());
    s1.files_touched = vec!["src/auth/login.rs".to_string()];

    let mut s2 = Session::new(
        "Add dashboard".to_string(),
        "claude-opus-4-5".to_string(),
        "orchestrator".to_string(),
        Some(102),
    );
    s2.id = Uuid::from_u128(1);
    s2.status = SessionStatus::Running;
    s2.started_at = Some(fixed_start());
    s2.finished_at = Some(fixed_end());
    s2.files_touched = vec!["src/tui/dashboard.rs".to_string()];

    (s1, s2)
}

fn scrub_noise(app: &mut crate::tui::app::App) {
    app.show_mascot = false;
    app.show_activity_log = false;
    app.spinner_tick = 0;
    app.gh_auth_ok = true;
    app.bypass_active = false;
    app.no_splash = true;
    app.status_bar_marquee = crate::tui::marquee::MarqueeState::new();
    app.status_bar_marquee_fingerprint = 0;
    // Pin RSS/total memory so the status-bar RAM widget is deterministic.
    app.resource_monitor = Box::new(crate::system::monitor::MockResourceMonitor::new(
        22 * 1024 * 1024,
        16 * 1024 * 1024 * 1024,
    ));
}

#[test]
fn agent_graph_dispatcher_toggle_on_renders_graph() {
    let mut app = make_test_app("dispatcher-toggle-on");
    app.config = Some(make_config(true));

    let (s1, s2) = two_agent_sessions();
    app.pool.enqueue(s1);
    app.pool.enqueue(s2);

    app.tui_mode = TuiMode::AgentGraph;
    scrub_noise(&mut app);

    let mut t = test_terminal();
    t.draw(|f| ui::draw(f, &mut app)).unwrap();

    let output = format!("{}", t.backend());
    assert!(
        output.contains("agent graph"),
        "toggle ON must render the graph canvas block title"
    );
    assert_snapshot!(t.backend());
}

#[test]
fn agent_graph_dispatcher_toggle_off_renders_panels() {
    let mut app = make_test_app("dispatcher-toggle-off");
    app.config = Some(make_config(false));

    let (s1, s2) = two_agent_sessions();
    app.pool.enqueue(s1);
    app.pool.enqueue(s2);

    app.tui_mode = TuiMode::AgentGraph;
    scrub_noise(&mut app);

    let mut t = test_terminal();
    t.draw(|f| ui::draw(f, &mut app)).unwrap();

    let output = format!("{}", t.backend());
    assert!(
        !output.contains("agent graph"),
        "toggle OFF must NOT render the graph canvas block title"
    );
    assert!(
        output.contains("#101"),
        "toggle OFF must render panel view showing session #101"
    );
    assert_snapshot!(t.backend());
}

#[test]
fn agent_graph_dispatcher_no_config_falls_back_to_panels() {
    let mut app = make_test_app("dispatcher-no-config");

    let (s1, s2) = two_agent_sessions();
    app.pool.enqueue(s1);
    app.pool.enqueue(s2);

    app.tui_mode = TuiMode::AgentGraph;
    scrub_noise(&mut app);

    let mut t = test_terminal();
    t.draw(|f| ui::draw(f, &mut app)).unwrap();

    let output = format!("{}", t.backend());
    assert!(
        !output.contains("agent graph"),
        "None config must fall back to panel view (not graph)"
    );
    assert!(
        output.contains("#101"),
        "None config must render panel view showing session #101"
    );
}
