//! Parity tests for the Layout tab schema-driven migration (issue #716).

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
use crate::tui::screens::settings::schema_tab::sync::sync_to_config;
use crate::tui::theme::Theme;
use crate::tui::widgets::WidgetKind;

const LAYOUT_TAB_INDEX: usize = 8;

const MINIMAL_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
    "[tui.layout]\nmode = \"vertical\"\ndensity = \"default\"\npreview_ratio = 50\nactivity_log_height = 25\n",
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

fn layout_table() -> &'static crate::config::schema::TableSchema {
    schema_for_config()
        .iter()
        .find(|t| t.name == "tui.layout")
        .expect("tui.layout schema must exist")
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
    "mode",
    "density",
    "preview_ratio",
    "activity_log_height",
];

#[test]
fn layout_tab_flag_off_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), flags_off());
    let fields = &screen.fields_per_tab[LAYOUT_TAB_INDEX];
    assert_eq!(fields.len(), 4);
    for (i, expected) in EXPECTED_LABELS.iter().enumerate() {
        assert_eq!(fields[i].widget.label(), *expected);
    }
}

#[test]
fn layout_tab_flag_on_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), flags_on());
    let fields = &screen.fields_per_tab[LAYOUT_TAB_INDEX];
    assert_eq!(fields.len(), 4);
    for (i, expected) in EXPECTED_LABELS.iter().enumerate() {
        assert_eq!(fields[i].widget.label(), *expected);
    }
}

#[test]
fn layout_tab_flag_off_renders_80x24() {
    let screen = SettingsScreen::new(test_config(), flags_off());
    let buf = render_tab(&screen.fields_per_tab[LAYOUT_TAB_INDEX], 80, 24);
    assert_snapshot!(format!("{buf:?}"));
}

#[test]
fn layout_tab_flag_on_renders_80x24_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), flags_off());
    let screen_on = SettingsScreen::new(test_config(), flags_on());
    let buf_off = render_tab(&screen_off.fields_per_tab[LAYOUT_TAB_INDEX], 80, 24);
    let buf_on = render_tab(&screen_on.fields_per_tab[LAYOUT_TAB_INDEX], 80, 24);
    assert_eq!(buf_off, buf_on);
}

#[test]
fn layout_tab_flag_on_renders_120x40_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), flags_off());
    let screen_on = SettingsScreen::new(test_config(), flags_on());
    let buf_off = render_tab(&screen_off.fields_per_tab[LAYOUT_TAB_INDEX], 120, 40);
    let buf_on = render_tab(&screen_on.fields_per_tab[LAYOUT_TAB_INDEX], 120, 40);
    assert_eq!(buf_off, buf_on);
}

#[test]
fn layout_sync_flag_on_writes_all_fields() -> anyhow::Result<()> {
    let mut config = test_config();
    let table = layout_table();
    let mut fields = from_schema(table, &config);

    if let WidgetKind::Dropdown(ref mut w) = fields[0].widget {
        w.selected = 1; // horizontal
    }
    if let WidgetKind::Dropdown(ref mut w) = fields[1].widget {
        w.selected = 2; // compact
    }
    if let WidgetKind::NumberStepper(ref mut w) = fields[2].widget {
        w.value = 70;
    }
    if let WidgetKind::NumberStepper(ref mut w) = fields[3].widget {
        w.value = 40;
    }

    sync_to_config(table, &fields, &mut config)?;

    assert!(matches!(
        config.tui.layout.mode,
        crate::config::LayoutMode::Horizontal
    ));
    assert!(matches!(
        config.tui.layout.density,
        crate::config::Density::Compact
    ));
    assert_eq!(config.tui.layout.preview_ratio, 70);
    assert_eq!(config.tui.layout.activity_log_height, 40);
    Ok(())
}

#[test]
fn layout_sync_flag_on_preserves_other_config_sections() -> anyhow::Result<()> {
    let original = test_config();
    let mut config = original.clone();
    let table = layout_table();
    let mut fields = from_schema(table, &config);
    if let WidgetKind::Dropdown(ref mut w) = fields[0].widget {
        w.selected = 1;
    }
    sync_to_config(table, &fields, &mut config)?;
    assert_eq!(config.project.repo, original.project.repo);
    assert_eq!(config.tui.ascii_icons, original.tui.ascii_icons);
    assert_eq!(config.tui.theme.preset, original.tui.theme.preset);
    Ok(())
}
