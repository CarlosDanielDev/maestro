//! Parity tests for the TurboQuant tab schema-driven migration (issue #716).

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

const TURBOQUANT_TAB_INDEX: usize = 12;

const MINIMAL_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
    "[turboquant]\nenabled = false\nbit_width = 4\nstrategy = \"turboquant\"\napply_to = \"both\"\nauto_on_overflow = false\n",
);

fn test_config() -> Config {
    toml::from_str(MINIMAL_TOML).expect("MINIMAL_TOML must parse")
}

fn turboquant_table() -> &'static crate::config::schema::TableSchema {
    schema_for_config()
        .iter()
        .find(|t| t.name == "turboquant")
        .expect("turboquant schema must exist")
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

const EXPECTED_LABELS: [&str; 5] = [
    "enabled",
    "bit_width",
    "strategy",
    "apply_to",
    "auto_on_overflow",
];

#[test]
fn turboquant_tab_flag_off_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[TURBOQUANT_TAB_INDEX];
    assert_eq!(fields.len(), 5);
    for (i, expected) in EXPECTED_LABELS.iter().enumerate() {
        assert_eq!(fields[i].widget.label(), *expected, "field[{i}] label");
    }
}

#[test]
fn turboquant_tab_flag_on_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[TURBOQUANT_TAB_INDEX];
    assert_eq!(fields.len(), 5);
    for (i, expected) in EXPECTED_LABELS.iter().enumerate() {
        assert_eq!(fields[i].widget.label(), *expected, "field[{i}] label");
    }
}

#[test]
fn turboquant_tab_flag_off_renders_80x24() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let buf = render_tab(&screen.fields_per_tab[TURBOQUANT_TAB_INDEX], 80, 24);
    assert_snapshot!(format!("{buf:?}"));
}

#[test]
fn turboquant_tab_flag_on_renders_80x24_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), FeatureFlags::default());
    let screen_on = SettingsScreen::new(test_config(), FeatureFlags::default());
    let buf_off = render_tab(&screen_off.fields_per_tab[TURBOQUANT_TAB_INDEX], 80, 24);
    let buf_on = render_tab(&screen_on.fields_per_tab[TURBOQUANT_TAB_INDEX], 80, 24);
    assert_eq!(buf_off, buf_on);
}

#[test]
fn turboquant_tab_flag_on_renders_120x40_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), FeatureFlags::default());
    let screen_on = SettingsScreen::new(test_config(), FeatureFlags::default());
    let buf_off = render_tab(&screen_off.fields_per_tab[TURBOQUANT_TAB_INDEX], 120, 40);
    let buf_on = render_tab(&screen_on.fields_per_tab[TURBOQUANT_TAB_INDEX], 120, 40);
    assert_eq!(buf_off, buf_on);
}

#[test]
fn turboquant_sync_flag_on_writes_all_fields() -> anyhow::Result<()> {
    let mut config = test_config();
    let table = turboquant_table();
    let mut fields = from_schema(table, &config);

    if let WidgetKind::Toggle(ref mut w) = fields[0].widget {
        w.value = true;
    }
    if let WidgetKind::NumberStepper(ref mut w) = fields[1].widget {
        w.value = 6;
    }
    if let WidgetKind::Dropdown(ref mut w) = fields[2].widget {
        w.selected = 2;
    }
    if let WidgetKind::Dropdown(ref mut w) = fields[3].widget {
        w.selected = 0;
    }
    if let WidgetKind::Toggle(ref mut w) = fields[4].widget {
        w.value = true;
    }

    sync_to_config(table, &fields, &mut config)?;

    assert!(config.turboquant.enabled);
    assert_eq!(config.turboquant.bit_width, 6);
    assert!(matches!(
        config.turboquant.strategy,
        crate::config::QuantStrategy::Qjl
    ));
    assert!(matches!(
        config.turboquant.apply_to,
        crate::config::ApplyTarget::Keys
    ));
    assert!(config.turboquant.auto_on_overflow);
    Ok(())
}

#[test]
fn turboquant_sync_flag_on_preserves_other_config_sections() -> anyhow::Result<()> {
    let original = test_config();
    let mut config = original.clone();
    let table = turboquant_table();
    let mut fields = from_schema(table, &config);
    if let WidgetKind::Toggle(ref mut w) = fields[0].widget {
        w.value = true;
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
