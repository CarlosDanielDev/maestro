//! Parity tests for the Advanced tab schema-driven migration (issue #716).
//!
//! Advanced is the only multi-table tab: three schema fields come from
//! two different tables (`concurrency` and `monitoring`) and the legacy
//! display order is `[heavy_task_limit, work_tick_interval_secs,
//! heavy_task_labels, caveman_mode]`. The `caveman_mode` toggle stays
//! bespoke (out of schema) and routes through the existing caveman
//! dispatch flow.

use insta::assert_snapshot;
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};

use crate::config::Config;
use crate::flags::store::FeatureFlags;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::SettingsScreen;
use crate::tui::theme::Theme;
use crate::tui::widgets::WidgetKind;

const ADVANCED_TAB_INDEX: usize = 13;
const CAVEMAN_LABEL: &str = "caveman_mode";

const MINIMAL_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
    "[concurrency]\nheavy_task_limit = 2\nheavy_task_labels = [\"heavy\", \"slow\"]\n",
    "[monitoring]\nwork_tick_interval_secs = 10\n",
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

const EXPECTED_LABELS: [&str; 4] = [
    "heavy_task_limit",
    "work_tick_interval_secs",
    "heavy_task_labels",
    CAVEMAN_LABEL,
];

#[test]
fn advanced_tab_flag_off_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[ADVANCED_TAB_INDEX];
    assert_eq!(fields.len(), 4);
    for (i, expected) in EXPECTED_LABELS.iter().enumerate() {
        assert_eq!(fields[i].widget.label(), *expected, "field[{i}] label");
    }
}

#[test]
fn advanced_tab_flag_on_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[ADVANCED_TAB_INDEX];
    assert_eq!(fields.len(), 4);
    for (i, expected) in EXPECTED_LABELS.iter().enumerate() {
        assert_eq!(fields[i].widget.label(), *expected, "field[{i}] label");
    }
}

#[test]
fn advanced_tab_flag_on_caveman_toggle_stays_at_index_three() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &screen.fields_per_tab[ADVANCED_TAB_INDEX];
    let WidgetKind::Toggle(t) = &fields[3].widget else {
        panic!("field[3] must be Toggle for caveman_mode");
    };
    assert_eq!(t.label, CAVEMAN_LABEL);
}

#[test]
fn advanced_tab_flag_off_renders_80x24() {
    let screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let buf = render_tab(&screen.fields_per_tab[ADVANCED_TAB_INDEX], 80, 24);
    assert_snapshot!(format!("{buf:?}"));
}

#[test]
fn advanced_tab_flag_on_renders_80x24_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), FeatureFlags::default());
    let screen_on = SettingsScreen::new(test_config(), FeatureFlags::default());
    let buf_off = render_tab(&screen_off.fields_per_tab[ADVANCED_TAB_INDEX], 80, 24);
    let buf_on = render_tab(&screen_on.fields_per_tab[ADVANCED_TAB_INDEX], 80, 24);
    assert_eq!(buf_off, buf_on);
}

#[test]
fn advanced_tab_flag_on_renders_120x40_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), FeatureFlags::default());
    let screen_on = SettingsScreen::new(test_config(), FeatureFlags::default());
    let buf_off = render_tab(&screen_off.fields_per_tab[ADVANCED_TAB_INDEX], 120, 40);
    let buf_on = render_tab(&screen_on.fields_per_tab[ADVANCED_TAB_INDEX], 120, 40);
    assert_eq!(buf_off, buf_on);
}

#[test]
fn advanced_sync_flag_on_writes_concurrency_and_monitoring_fields() {
    let mut screen = SettingsScreen::new(test_config(), FeatureFlags::default());
    let fields = &mut screen.fields_per_tab[ADVANCED_TAB_INDEX];

    if let WidgetKind::NumberStepper(ref mut w) = fields[0].widget {
        w.value = 4;
    }
    if let WidgetKind::NumberStepper(ref mut w) = fields[1].widget {
        w.value = 30;
    }
    if let WidgetKind::ListEditor(ref mut w) = fields[2].widget {
        w.items = vec!["x".into(), "y".into(), "z".into()];
    }

    screen.sync_widgets_to_config();

    assert_eq!(screen.config.concurrency.heavy_task_limit, 4);
    assert_eq!(screen.config.monitoring.work_tick_interval_secs, 30);
    assert_eq!(
        screen.config.concurrency.heavy_task_labels,
        vec!["x".to_string(), "y".to_string(), "z".to_string()]
    );
}

#[test]
fn advanced_sync_flag_on_preserves_other_config_sections() {
    let original = test_config();
    let mut screen = SettingsScreen::new(original.clone(), FeatureFlags::default());
    let fields = &mut screen.fields_per_tab[ADVANCED_TAB_INDEX];
    if let WidgetKind::NumberStepper(ref mut w) = fields[0].widget {
        w.value = 5;
    }
    screen.sync_widgets_to_config();
    assert_eq!(screen.config.project.repo, original.project.repo);
    assert_eq!(
        screen.config.sessions.max_concurrent,
        original.sessions.max_concurrent
    );
    assert_eq!(screen.config.review.command, original.review.command);
}
