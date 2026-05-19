//! Field-shape regression tests for the Budget tab after migration to the
//! schema-driven renderer (#785). Pins field count, labels, render bytes at
//! 80×24 + 120×40, schema sync writeback, and round-trip of fractional
//! values (per_session_usd, total_usd).

use insta::assert_snapshot;
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};

use crate::config::Config;
use crate::config::schema::BUDGET_TABLE;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;
use crate::tui::screens::settings::schema_tab::sync::sync_to_config;
use crate::tui::theme::Theme;
use crate::tui::widgets::WidgetKind;

const MINIMAL_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.5\ntotal_usd = 12.5\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
);

fn test_config() -> Config {
    toml::from_str(MINIMAL_TOML).expect("MINIMAL_TOML must parse")
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
fn budget_tab_field_count_and_labels() {
    let fields = from_schema(&BUDGET_TABLE, &test_config());
    assert_eq!(fields.len(), 3, "budget tab must have exactly 3 fields");
    assert_eq!(fields[0].widget.label(), "per_session_usd");
    assert_eq!(fields[1].widget.label(), "total_usd");
    assert_eq!(fields[2].widget.label(), "alert_threshold_pct");
}

#[test]
fn budget_tab_renders_80x24() {
    let fields = from_schema(&BUDGET_TABLE, &test_config());
    let buf = render_tab(&fields, 80, 24);
    assert_snapshot!(format!("{buf:?}"));
}

#[test]
fn budget_tab_renders_120x40() {
    let fields = from_schema(&BUDGET_TABLE, &test_config());
    let buf = render_tab(&fields, 120, 40);
    assert_snapshot!(format!("{buf:?}"));
}

#[test]
fn budget_sync_writes_fractional_per_session_usd_to_config() -> anyhow::Result<()> {
    let mut config = test_config();
    let mut fields = from_schema(&BUDGET_TABLE, &config);
    if let WidgetKind::NumberStepper(ref mut w) = fields[0].widget {
        w.value = 75;
    } else {
        anyhow::bail!("field[0] must be NumberStepper for per_session_usd");
    }
    sync_to_config(&BUDGET_TABLE, &fields, &mut config)?;
    assert!(
        (config.budget.per_session_usd - 7.5).abs() < 1e-9,
        "per_session_usd must be 7.5, got {}",
        config.budget.per_session_usd
    );
    Ok(())
}

#[test]
fn budget_sync_writes_fractional_total_usd_to_config() -> anyhow::Result<()> {
    let mut config = test_config();
    let mut fields = from_schema(&BUDGET_TABLE, &config);
    if let WidgetKind::NumberStepper(ref mut w) = fields[1].widget {
        w.value = 125;
    } else {
        anyhow::bail!("field[1] must be NumberStepper for total_usd");
    }
    sync_to_config(&BUDGET_TABLE, &fields, &mut config)?;
    assert!(
        (config.budget.total_usd - 12.5).abs() < 1e-9,
        "total_usd must be 12.5, got {}",
        config.budget.total_usd
    );
    Ok(())
}

#[test]
fn budget_sync_preserves_other_config_sections() -> anyhow::Result<()> {
    let original = test_config();
    let mut config = original.clone();
    let mut fields = from_schema(&BUDGET_TABLE, &config);
    if let WidgetKind::NumberStepper(ref mut w) = fields[0].widget {
        w.value = 100;
    }
    sync_to_config(&BUDGET_TABLE, &fields, &mut config)?;
    assert_eq!(
        config.project.repo, original.project.repo,
        "sync must not disturb project.repo"
    );
    assert_eq!(
        config.sessions.max_concurrent, original.sessions.max_concurrent,
        "sync must not disturb sessions.max_concurrent"
    );
    assert_eq!(
        config.github.auto_pr, original.github.auto_pr,
        "sync must not disturb github.auto_pr"
    );
    Ok(())
}

#[test]
fn budget_round_trip_per_session_5point5_total_12point5() {
    let original = test_config();
    let toml_str = toml::to_string(&original).expect("Config must serialize");
    let round_tripped: Config = toml::from_str(&toml_str).expect("round-tripped TOML must parse");
    assert_eq!(
        round_tripped.budget.per_session_usd, original.budget.per_session_usd,
        "per_session_usd byte-equal after round-trip"
    );
    assert_eq!(
        round_tripped.budget.total_usd, original.budget.total_usd,
        "total_usd byte-equal after round-trip"
    );
    assert_eq!(
        round_tripped.budget.alert_threshold_pct, original.budget.alert_threshold_pct,
        "alert_threshold_pct byte-equal after round-trip"
    );
}

#[test]
fn budget_full_round_trip_through_schema_renderer() -> anyhow::Result<()> {
    let original = test_config();
    let mut config = original.clone();
    let fields = from_schema(&BUDGET_TABLE, &config);
    sync_to_config(&BUDGET_TABLE, &fields, &mut config)?;
    assert!(
        (config.budget.per_session_usd - 5.5).abs() < 1e-9,
        "per_session_usd must round-trip to 5.5 through schema, got {}",
        config.budget.per_session_usd
    );
    assert!(
        (config.budget.total_usd - 12.5).abs() < 1e-9,
        "total_usd must round-trip to 12.5 through schema, got {}",
        config.budget.total_usd
    );
    Ok(())
}
