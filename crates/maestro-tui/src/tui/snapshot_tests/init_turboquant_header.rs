//! #1162: a project fresh from `maestro init` shows `TQ:ON` in the header.

use insta::assert_snapshot;

use crate::config::Config;
use crate::init::DetectedStack;
use crate::tui::snapshot_tests::test_terminal;
use crate::tui::{make_test_app, ui};

#[test]
fn header_shows_tq_on_for_freshly_initialised_project() {
    let rendered = crate::init::template::render(&[DetectedStack::Rust]);
    let config: Config = toml::from_str(&rendered).expect("rendered template parses");

    let mut app = make_test_app("init-turboquant-header");
    // Same flag sync as `App::configure`; the full call also touches
    // process-wide icon mode and on-disk prompt history.
    if config.turboquant.enabled {
        app.flags.set_enabled(crate::flags::Flag::TurboQuant, true);
    }
    app.config = Some(config);
    app.show_mascot = false;
    app.show_activity_log = false;
    app.spinner_tick = 0;
    app.gh_auth_ok = true;
    app.bypass_active = false;
    app.no_splash = true;
    app.resource_monitor = Box::new(crate::system::monitor::MockResourceMonitor::new(
        22 * 1024 * 1024,
        16 * 1024 * 1024 * 1024,
    ));

    let mut t = test_terminal();
    t.draw(|f| ui::draw(f, &mut app)).unwrap();

    let header: String = format!("{}", t.backend())
        .lines()
        .take(3)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(header.contains("TQ:ON"), "{header}");
    assert_snapshot!(header);
}
