//! Parity tests for the Gates tab schema-driven migration (issue #716).
//!
//! Gates is the first tab whose schema reshape collapsed a sibling
//! `TableSchema` (`gates.ci_auto_fix`) into a `NestedTable` child of
//! `GATES_FIELDS`. The legacy hand-coded labels (`ci_auto_fix.enabled`,
//! `ci_auto_fix.max_retries`) already use the dotted form, so the schema
//! emits byte-identical labels.

use insta::assert_snapshot;
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};

use crate::config::Config;
use crate::config::schema::schema_for_config;
use crate::flags::store::FeatureFlags;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::SettingsScreen;
use crate::tui::screens::settings::schema_tab::build::from_schema;
use crate::tui::screens::settings::schema_tab::sync::sync_to_config;
use crate::tui::theme::Theme;
use crate::tui::widgets::WidgetKind;

const GATES_TAB_INDEX: usize = 5;

const MINIMAL_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
    "[gates]\nenabled = true\ntest_command = \"cargo test\"\nci_poll_interval_secs = 30\nci_max_wait_secs = 1800\n",
    "[gates.ci_auto_fix]\nenabled = true\nmax_retries = 3\n",
);

fn test_config() -> Config {
    toml::from_str(MINIMAL_TOML).expect("MINIMAL_TOML must parse")
}

fn gates_table() -> &'static crate::config::schema::TableSchema {
    schema_for_config()
        .iter()
        .find(|t| t.name == "gates")
        .expect("gates schema must exist")
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

const EXPECTED_LABELS: [&str; 6] = [
    "enabled",
    "test_command",
    "ci_poll_interval_secs",
    "ci_max_wait_secs",
    "ci_auto_fix.enabled",
    "ci_auto_fix.max_retries",
];

#[test]
fn gates_tab_flag_off_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[GATES_TAB_INDEX];
    assert_eq!(fields.len(), 6);
    for (i, expected) in EXPECTED_LABELS.iter().enumerate() {
        assert_eq!(fields[i].widget.label(), *expected);
    }
}

#[test]
fn gates_tab_flag_on_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[GATES_TAB_INDEX];
    assert_eq!(fields.len(), 6);
    for (i, expected) in EXPECTED_LABELS.iter().enumerate() {
        assert_eq!(fields[i].widget.label(), *expected);
    }
}

#[test]
fn gates_tab_flag_off_renders_80x24() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let buf = render_tab(&screen.fields_per_tab[GATES_TAB_INDEX], 80, 24);
    assert_snapshot!(format!("{buf:?}"));
}

#[test]
fn gates_tab_flag_on_renders_80x24_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), FeatureFlags::default());
    let screen_on = SettingsScreen::new(test_config(), FeatureFlags::default());
    let buf_off = render_tab(&screen_off.fields_per_tab[GATES_TAB_INDEX], 80, 24);
    let buf_on = render_tab(&screen_on.fields_per_tab[GATES_TAB_INDEX], 80, 24);
    assert_eq!(buf_off, buf_on);
}

#[test]
fn gates_tab_flag_on_renders_120x40_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), FeatureFlags::default());
    let screen_on = SettingsScreen::new(test_config(), FeatureFlags::default());
    let buf_off = render_tab(&screen_off.fields_per_tab[GATES_TAB_INDEX], 120, 40);
    let buf_on = render_tab(&screen_on.fields_per_tab[GATES_TAB_INDEX], 120, 40);
    assert_eq!(buf_off, buf_on);
}

#[test]
fn gates_sync_flag_on_writes_outer_and_nested_fields() -> anyhow::Result<()> {
    let mut config = test_config();
    let table = gates_table();
    let mut fields = from_schema(table, &config);

    if let WidgetKind::Toggle(ref mut w) = fields[0].widget {
        w.value = false;
    }
    if let WidgetKind::TextInput(ref mut w) = fields[1].widget {
        w.value = "cargo nextest run".into();
    }
    if let WidgetKind::NumberStepper(ref mut w) = fields[2].widget {
        w.value = 60;
    }
    if let WidgetKind::NumberStepper(ref mut w) = fields[3].widget {
        w.value = 3600;
    }
    if let WidgetKind::Toggle(ref mut w) = fields[4].widget {
        w.value = false;
    }
    if let WidgetKind::NumberStepper(ref mut w) = fields[5].widget {
        w.value = 5;
    }

    sync_to_config(table, &fields, &mut config)?;

    assert!(!config.gates.enabled);
    assert_eq!(config.gates.test_command, "cargo nextest run");
    assert_eq!(config.gates.ci_poll_interval_secs, 60);
    assert_eq!(config.gates.ci_max_wait_secs, 3600);
    assert!(!config.gates.ci_auto_fix.enabled);
    assert_eq!(config.gates.ci_auto_fix.max_retries, 5);
    Ok(())
}

#[test]
fn gates_sync_flag_on_preserves_other_config_sections() -> anyhow::Result<()> {
    let original = test_config();
    let mut config = original.clone();
    let table = gates_table();
    let mut fields = from_schema(table, &config);
    if let WidgetKind::Toggle(ref mut w) = fields[0].widget {
        w.value = false;
    }
    sync_to_config(table, &fields, &mut config)?;
    assert_eq!(config.project.repo, original.project.repo);
    assert_eq!(
        config.sessions.max_concurrent,
        original.sessions.max_concurrent
    );
    assert_eq!(config.review.command, original.review.command);
    Ok(())
}
