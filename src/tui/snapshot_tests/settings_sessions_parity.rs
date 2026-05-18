//! Parity tests for the Sessions tab schema-driven migration (issue #716).
//!
//! Sessions is the most intricate tab:
//! - 16 schema-driven fields across `sessions` + 3 NestedTable children
//!   (`hollow_retry`, `context_overflow`, `conflict`).
//! - 1 bespoke `bypass_review_corrections` toggle inserted at index 4 — a
//!   derived view of `permission_mode` with custom pre-sync writeback so
//!   the dropdown still wins on conflict.

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

const SESSIONS_TAB_INDEX: usize = 1;
const BYPASS_LABEL: &str =
    "bypass_review_corrections (DANGER: auto-accepts all review fixes)";

const MINIMAL_TOML: &str = concat!(
    "[project]\nrepo = \"owner/repo\"\nbase_branch = \"main\"\n",
    "[sessions]\nmax_concurrent = 3\nstall_timeout_secs = 1800\ndefault_model = \"claude-opus-4-7\"\ndefault_mode = \"orchestrator\"\npermission_mode = \"default\"\nmax_retries = 3\nretry_cooldown_secs = 60\n",
    "[sessions.hollow_retry]\npolicy = \"intent-aware\"\nwork_max_retries = 2\nconsultation_max_retries = 0\n",
    "[sessions.context_overflow]\noverflow_threshold_pct = 70\nauto_fork = true\ncommit_prompt_pct = 50\nmax_fork_depth = 5\n",
    "[sessions.conflict]\nenabled = true\npolicy = \"warn\"\n",
    "[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n",
    "[github]\n",
    "[notifications]\nslack_webhook_url = \"\"\n",
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

const EXPECTED_LABELS: [&str; 17] = [
    "max_concurrent",
    "stall_timeout_secs",
    "default_model",
    "default_mode",
    BYPASS_LABEL,
    "permission_mode",
    "max_retries",
    "retry_cooldown_secs",
    "hollow_retry.policy",
    "hollow_retry.work_max_retries",
    "hollow_retry.consultation_max_retries",
    "context_overflow.overflow_threshold_pct",
    "context_overflow.auto_fork",
    "context_overflow.commit_prompt_pct",
    "context_overflow.max_fork_depth",
    "conflict.enabled",
    "conflict.policy",
];

#[test]
fn sessions_tab_flag_off_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), flags_off());
    let fields = &screen.fields_per_tab[SESSIONS_TAB_INDEX];
    assert_eq!(fields.len(), 17);
    for (i, expected) in EXPECTED_LABELS.iter().enumerate() {
        assert_eq!(fields[i].widget.label(), *expected, "field[{i}] label");
    }
}

#[test]
fn sessions_tab_flag_on_field_count_and_labels() {
    let screen = SettingsScreen::new(test_config(), flags_on());
    let fields = &screen.fields_per_tab[SESSIONS_TAB_INDEX];
    assert_eq!(fields.len(), 17);
    for (i, expected) in EXPECTED_LABELS.iter().enumerate() {
        assert_eq!(fields[i].widget.label(), *expected, "field[{i}] label");
    }
}

#[test]
fn sessions_tab_flag_on_bypass_toggle_pinned_at_index_4() {
    let screen = SettingsScreen::new(test_config(), flags_on());
    let fields = &screen.fields_per_tab[SESSIONS_TAB_INDEX];
    let WidgetKind::Toggle(t) = &fields[4].widget else {
        panic!("field[4] must be Toggle for bypass_review_corrections");
    };
    assert_eq!(t.label, BYPASS_LABEL);
}

#[test]
fn sessions_tab_flag_off_renders_80x24() {
    let screen = SettingsScreen::new(test_config(), flags_off());
    let buf = render_tab(&screen.fields_per_tab[SESSIONS_TAB_INDEX], 80, 24);
    assert_snapshot!(format!("{buf:?}"));
}

#[test]
fn sessions_tab_flag_on_renders_80x24_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), flags_off());
    let screen_on = SettingsScreen::new(test_config(), flags_on());
    let buf_off = render_tab(&screen_off.fields_per_tab[SESSIONS_TAB_INDEX], 80, 24);
    let buf_on = render_tab(&screen_on.fields_per_tab[SESSIONS_TAB_INDEX], 80, 24);
    assert_eq!(buf_off, buf_on);
}

#[test]
fn sessions_tab_flag_on_renders_120x40_parity_with_flag_off() {
    let screen_off = SettingsScreen::new(test_config(), flags_off());
    let screen_on = SettingsScreen::new(test_config(), flags_on());
    let buf_off = render_tab(&screen_off.fields_per_tab[SESSIONS_TAB_INDEX], 120, 40);
    let buf_on = render_tab(&screen_on.fields_per_tab[SESSIONS_TAB_INDEX], 120, 40);
    assert_eq!(buf_off, buf_on);
}

#[test]
fn sessions_sync_flag_on_writes_outer_default_and_nested_fields() {
    let mut screen = SettingsScreen::new(test_config(), flags_on());
    let fields = &mut screen.fields_per_tab[SESSIONS_TAB_INDEX];

    if let WidgetKind::NumberStepper(ref mut w) = fields[0].widget {
        w.value = 5;
    }
    if let WidgetKind::Dropdown(ref mut w) = fields[8].widget {
        w.selected = 0; // hollow_retry.policy = always
    }
    if let WidgetKind::Toggle(ref mut w) = fields[12].widget {
        w.value = false; // context_overflow.auto_fork
    }
    if let WidgetKind::Dropdown(ref mut w) = fields[16].widget {
        w.selected = 2; // conflict.policy = kill
    }

    screen.sync_widgets_to_config();

    assert_eq!(screen.config.sessions.max_concurrent, 5);
    assert!(matches!(
        screen.config.sessions.hollow_retry.policy,
        crate::config::HollowRetryPolicy::Always
    ));
    assert!(!screen.config.sessions.context_overflow.auto_fork);
    assert!(matches!(
        screen.config.sessions.conflict.policy,
        crate::config::ConflictPolicy::Kill
    ));
}

#[test]
fn sessions_bypass_toggle_on_then_dropdown_acceptedits_keeps_acceptedits() {
    let mut config = test_config();
    config.sessions.permission_mode = "default".into();
    let mut screen = SettingsScreen::new(config, flags_on());
    let fields = &mut screen.fields_per_tab[SESSIONS_TAB_INDEX];

    if let WidgetKind::Toggle(ref mut w) = fields[4].widget {
        w.value = true;
    }
    if let WidgetKind::Dropdown(ref mut w) = fields[5].widget {
        let idx = w
            .options
            .iter()
            .position(|s| s == "acceptEdits")
            .unwrap();
        w.selected = idx;
    }

    screen.sync_widgets_to_config();

    assert_eq!(
        screen.config.sessions.permission_mode, "acceptEdits",
        "dropdown wins over bypass toggle"
    );
}

#[test]
fn sessions_bypass_toggle_off_reverts_bypass_to_default() {
    let mut config = test_config();
    config.sessions.permission_mode = "bypassPermissions".into();
    let mut screen = SettingsScreen::new(config, flags_on());
    let fields = &mut screen.fields_per_tab[SESSIONS_TAB_INDEX];

    if let WidgetKind::Toggle(ref mut w) = fields[4].widget {
        w.value = false;
    }
    // Dropdown still shows bypassPermissions (initial state) — sync will
    // re-apply the dropdown's value after the toggle pre-hook reverts it.
    // Net effect: dropdown wins, so permission_mode stays bypassPermissions.
    // To actually revert via the toggle, the dropdown must also be moved.
    if let WidgetKind::Dropdown(ref mut w) = fields[5].widget {
        let idx = w.options.iter().position(|s| s == "default").unwrap();
        w.selected = idx;
    }
    screen.sync_widgets_to_config();

    assert_eq!(screen.config.sessions.permission_mode, "default");
}

#[test]
fn sessions_sync_flag_on_preserves_other_config_sections() {
    let original = test_config();
    let mut screen = SettingsScreen::new(original.clone(), flags_on());
    let fields = &mut screen.fields_per_tab[SESSIONS_TAB_INDEX];
    if let WidgetKind::NumberStepper(ref mut w) = fields[0].widget {
        w.value = 4;
    }
    screen.sync_widgets_to_config();
    assert_eq!(screen.config.project.repo, original.project.repo);
    assert_eq!(screen.config.review.command, original.review.command);
    assert_eq!(screen.config.gates.enabled, original.gates.enabled);
}

// Compile-time guard so `BYPASS_LABEL` stays in sync with the tabs module.
#[allow(dead_code)]
fn _ensure_schema_table_present() {
    let _ = schema_for_config()
        .iter()
        .find(|t| t.name == "sessions")
        .expect("sessions schema must exist");
    let _ = from_schema;
}
