//! Field-shape regression tests for the Review tab schema migration
//! (issue #716). Pins field count, labels, render bytes at 80×24 +
//! 120×40, and the schema sync writeback.

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

const REVIEW_TAB_INDEX: usize = 6;

const MINIMAL_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
    "[review]\nenabled = true\ncommand = \"cargo test\"\n",
);

fn test_config() -> Config {
    toml::from_str(MINIMAL_TOML).expect("MINIMAL_TOML must parse")
}

fn review_table() -> &'static crate::config::schema::TableSchema {
    schema_for_config()
        .iter()
        .find(|t| t.name == "review")
        .expect("review schema must exist")
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

#[test]
fn review_tab_flag_off_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[REVIEW_TAB_INDEX];

    assert_eq!(
        fields.len(),
        3,
        "review tab must have enabled + command + reviewers VecOfStruct"
    );
    assert_eq!(fields[0].widget.label(), "enabled");
    assert_eq!(fields[1].widget.label(), "command");
    assert_eq!(fields[2].widget.label(), "reviewers");
    assert!(
        matches!(fields[2].widget, WidgetKind::DynamicRows(_)),
        "reviewers must render as DynamicRows"
    );
}

#[test]
fn review_tab_flag_on_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[REVIEW_TAB_INDEX];

    assert_eq!(
        fields.len(),
        3,
        "review tab must have 3 fields (schema-on path)"
    );
    assert_eq!(fields[0].widget.label(), "enabled");
    assert_eq!(fields[1].widget.label(), "command");
    assert_eq!(fields[2].widget.label(), "reviewers");
}

#[test]
fn review_tab_flag_off_renders_80x24() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let buf = render_tab(&screen.fields_per_tab[REVIEW_TAB_INDEX], 80, 24);
    assert_snapshot!(format!("{buf:?}"));
}

#[test]
fn review_tab_flag_on_renders_80x24_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), FeatureFlags::default());
    let screen_on = SettingsScreen::new(test_config(), FeatureFlags::default());

    let buf_off = render_tab(&screen_off.fields_per_tab[REVIEW_TAB_INDEX], 80, 24);
    let buf_on = render_tab(&screen_on.fields_per_tab[REVIEW_TAB_INDEX], 80, 24);

    assert_eq!(
        buf_off, buf_on,
        "schema-on and schema-off must produce byte-identical 80×24 render"
    );
}

#[test]
fn review_tab_flag_on_renders_120x40_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), FeatureFlags::default());
    let screen_on = SettingsScreen::new(test_config(), FeatureFlags::default());

    let buf_off = render_tab(&screen_off.fields_per_tab[REVIEW_TAB_INDEX], 120, 40);
    let buf_on = render_tab(&screen_on.fields_per_tab[REVIEW_TAB_INDEX], 120, 40);

    assert_eq!(
        buf_off, buf_on,
        "schema-on and schema-off must produce byte-identical 120×40 render"
    );
}

#[test]
fn review_sync_flag_on_writes_enabled_and_command() -> anyhow::Result<()> {
    let mut config = test_config();
    let table = review_table();
    let mut fields = from_schema(table, &config);

    if let WidgetKind::Toggle(ref mut w) = fields[0].widget {
        w.value = false;
    } else {
        anyhow::bail!("field[0] must be Toggle for enabled");
    }
    if let WidgetKind::TextInput(ref mut w) = fields[1].widget {
        w.value = "cargo nextest run".to_string();
    } else {
        anyhow::bail!("field[1] must be TextInput for command");
    }

    sync_to_config(table, &fields, &mut config)?;

    assert!(!config.review.enabled);
    assert_eq!(config.review.command, "cargo nextest run");
    Ok(())
}

#[test]
fn review_sync_flag_on_preserves_other_config_sections() -> anyhow::Result<()> {
    let original = test_config();
    let mut config = original.clone();
    let table = review_table();
    let mut fields = from_schema(table, &config);

    if let WidgetKind::TextInput(ref mut w) = fields[1].widget {
        w.value = "make test".to_string();
    }

    sync_to_config(table, &fields, &mut config)?;

    assert_eq!(
        config.sessions.max_concurrent, original.sessions.max_concurrent,
        "sync must not disturb sessions.max_concurrent"
    );
    assert_eq!(
        config.project.repo, original.project.repo,
        "sync must not disturb project.repo"
    );
    assert_eq!(
        config.github.auto_pr, original.github.auto_pr,
        "sync must not disturb github.auto_pr"
    );
    Ok(())
}
