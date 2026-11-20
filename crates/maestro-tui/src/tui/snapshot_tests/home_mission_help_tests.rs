//! #1384: Help on Home names only the keys that act at this width.

use super::{busy_app, draw, rows};

/// Help names `Tab` only where Tab moves focus (not on Compact).
#[test]
fn home_help_offers_tab_only_when_actions_show() {
    for (w, h, shown) in [(80, 24, false), (120, 36, true)] {
        let mut app = busy_app();
        draw(&mut app, w, h);
        app.help_state = Some(crate::tui::help::HelpOverlayState::new());
        let text = rows(&draw(&mut app, w, h)).join("\n");
        assert!(text.contains("Help"), "{text}");
        assert_eq!(text.contains("Switch Needs you"), shown, "{w}x{h}:\n{text}");
    }
}
