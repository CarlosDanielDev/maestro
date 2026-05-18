//! Parity tests for the Notifications tab schema-driven migration (issue #716).

use insta::assert_snapshot;
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};

use crate::config::Config;
use crate::config::schema::schema_for_config;
use crate::flags::Flag;
use crate::flags::store::FeatureFlags;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::SettingsScreen;
use crate::tui::screens::settings::schema_tab::build::from_schema;
use crate::tui::theme::Theme;
use crate::tui::widgets::WidgetKind;

const NOTIFICATIONS_TAB_INDEX: usize = 4;

const MINIMAL_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\ndesktop = true\nslack = false\nslack_webhook_url = \"\"\nslack_rate_limit_per_min = 5\n",
);

fn test_config() -> Config {
    toml::from_str(MINIMAL_TOML).expect("MINIMAL_TOML must parse")
}

fn flags_off() -> FeatureFlags {
    FeatureFlags::default()
}

fn flags_on() -> FeatureFlags {
    let mut f = FeatureFlags::default();
    f.set_enabled(Flag::SchemaDrivenSettings, true);
    f
}

fn render_tab(fields: &[SettingsField], width: u16, height: u16) -> ratatui::buffer::Buffer {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("TestBackend must init");
    let theme = Theme::dark();
    terminal
        .draw(|f| {
            let area = f.area();
            for (i, field) in fields.iter().enumerate() {
                let y = i as u16;
                if y >= area.height {
                    break;
                }
                let row = Rect {
                    x: area.x,
                    y: area.y + y,
                    width: area.width,
                    height: 1,
                };
                field.widget.draw(f, row, &theme, i == 0, None);
            }
        })
        .expect("draw must succeed");
    terminal.backend().buffer().clone()
}

const EXPECTED_LABELS: [&str; 4] = [
    "desktop",
    "slack",
    "slack_webhook_url",
    "slack_rate_limit_per_min",
];

#[test]
fn notifications_tab_flag_off_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), flags_off());
    let fields = &screen.fields_per_tab[NOTIFICATIONS_TAB_INDEX];
    assert_eq!(fields.len(), 4);
    for (i, expected) in EXPECTED_LABELS.iter().enumerate() {
        assert_eq!(fields[i].widget.label(), *expected);
    }
}

#[test]
fn notifications_tab_flag_on_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), flags_on());
    let fields = &screen.fields_per_tab[NOTIFICATIONS_TAB_INDEX];
    assert_eq!(fields.len(), 4);
    for (i, expected) in EXPECTED_LABELS.iter().enumerate() {
        assert_eq!(fields[i].widget.label(), *expected);
    }
}

#[test]
fn notifications_tab_flag_off_renders_80x24() {
    let screen = SettingsScreen::new(test_config(), flags_off());
    let buf = render_tab(&screen.fields_per_tab[NOTIFICATIONS_TAB_INDEX], 80, 24);
    assert_snapshot!(format!("{buf:?}"));
}

#[test]
fn notifications_tab_flag_on_renders_80x24_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), flags_off());
    let screen_on = SettingsScreen::new(test_config(), flags_on());
    let buf_off = render_tab(&screen_off.fields_per_tab[NOTIFICATIONS_TAB_INDEX], 80, 24);
    let buf_on = render_tab(&screen_on.fields_per_tab[NOTIFICATIONS_TAB_INDEX], 80, 24);
    assert_eq!(buf_off, buf_on);
}

#[test]
fn notifications_tab_flag_on_renders_120x40_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), flags_off());
    let screen_on = SettingsScreen::new(test_config(), flags_on());
    let buf_off = render_tab(&screen_off.fields_per_tab[NOTIFICATIONS_TAB_INDEX], 120, 40);
    let buf_on = render_tab(&screen_on.fields_per_tab[NOTIFICATIONS_TAB_INDEX], 120, 40);
    assert_eq!(buf_off, buf_on);
}

#[test]
fn notifications_sync_flag_on_writes_all_fields() {
    let mut config = test_config();
    config.notifications.slack_webhook_url = Some("https://hooks.example/legacy".to_string());

    let mut screen = SettingsScreen::new(config, flags_on());
    let fields = &mut screen.fields_per_tab[NOTIFICATIONS_TAB_INDEX];

    if let WidgetKind::Toggle(ref mut w) = fields[0].widget {
        w.value = false;
    }
    if let WidgetKind::Toggle(ref mut w) = fields[1].widget {
        w.value = true;
    }
    if let WidgetKind::TextInput(ref mut w) = fields[2].widget {
        w.value = "https://hooks.example/new".into();
    }
    if let WidgetKind::NumberStepper(ref mut w) = fields[3].widget {
        w.value = 30;
    }
    screen.sync_widgets_to_config();

    assert!(!screen.config.notifications.desktop);
    assert!(screen.config.notifications.slack);
    assert_eq!(
        screen.config.notifications.slack_webhook_url.as_deref(),
        Some("https://hooks.example/new")
    );
    assert_eq!(screen.config.notifications.slack_rate_limit_per_min, 30);
}

#[test]
fn notifications_sync_flag_on_empty_slack_url_collapses_to_none() {
    let mut config = test_config();
    config.notifications.slack_webhook_url = Some("https://hooks.example/legacy".to_string());

    let mut screen = SettingsScreen::new(config, flags_on());
    let fields = &mut screen.fields_per_tab[NOTIFICATIONS_TAB_INDEX];

    if let WidgetKind::TextInput(ref mut w) = fields[2].widget {
        w.value = String::new();
    }
    screen.sync_widgets_to_config();

    assert_eq!(
        screen.config.notifications.slack_webhook_url, None,
        "empty slack_webhook_url must collapse to None to preserve legacy semantics"
    );
}

#[test]
fn notifications_sync_flag_on_preserves_other_config_sections() -> anyhow::Result<()> {
    let original = test_config();
    let mut screen = SettingsScreen::new(original.clone(), flags_on());
    let fields = &mut screen.fields_per_tab[NOTIFICATIONS_TAB_INDEX];
    if let WidgetKind::Toggle(ref mut w) = fields[0].widget {
        w.value = false;
    }
    screen.sync_widgets_to_config();

    assert_eq!(screen.config.project.repo, original.project.repo);
    assert_eq!(
        screen.config.sessions.max_concurrent,
        original.sessions.max_concurrent
    );
    assert_eq!(screen.config.review.command, original.review.command);
    Ok(())
}

// Compile-time guard so future renames stay consistent across files.
#[allow(dead_code)]
fn _ensure_schema_table_present() {
    let _ = schema_for_config()
        .iter()
        .find(|t| t.name == "notifications")
        .expect("notifications schema must exist");
    let _ = from_schema;
}
