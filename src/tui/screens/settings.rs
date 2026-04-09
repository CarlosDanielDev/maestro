use crate::config::Config;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::screens::{ScreenAction, draw_keybinds_bar};
use crate::tui::theme::Theme;
use crate::tui::widgets::{
    Dropdown, ListEditor, NumberStepper, TextInput, Toggle, WidgetAction, WidgetKind,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::Screen;

/// Tab sections in the settings screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Project,
    Sessions,
    Budget,
    GitHub,
    Notifications,
    Gates,
    Review,
    Theme,
    Layout,
    Advanced,
}

impl SettingsTab {
    pub const ALL: &'static [SettingsTab] = &[
        Self::Project,
        Self::Sessions,
        Self::Budget,
        Self::GitHub,
        Self::Notifications,
        Self::Gates,
        Self::Review,
        Self::Theme,
        Self::Layout,
        Self::Advanced,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Sessions => "Sessions",
            Self::Budget => "Budget",
            Self::GitHub => "GitHub",
            Self::Notifications => "Notifications",
            Self::Gates => "Gates",
            Self::Review => "Review",
            Self::Theme => "Theme",
            Self::Layout => "Layout",
            Self::Advanced => "Advanced",
        }
    }
}

/// A single field in a settings tab, pairing a label with a widget.
pub struct SettingsField {
    pub widget: WidgetKind,
}

pub struct SettingsScreen {
    pub config: Config,
    original_config: Config,
    pub config_path: Option<std::path::PathBuf>,
    active_tab: usize,
    field_index: usize,
    fields_per_tab: Vec<Vec<SettingsField>>,
    scroll_offset: usize,
    confirm_discard: bool,
    save_flash: Option<std::time::Instant>,
    pub live_preview: bool,
}

impl SettingsScreen {
    pub fn new(config: Config) -> Self {
        let fields_per_tab = Self::build_fields(&config);
        Self {
            original_config: config.clone(),
            config,
            config_path: None,
            active_tab: 0,
            field_index: 0,
            fields_per_tab,
            scroll_offset: 0,
            confirm_discard: false,
            save_flash: None,
            live_preview: false,
        }
    }

    pub fn with_config_path(mut self, path: std::path::PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    pub fn is_dirty(&self) -> bool {
        self.config != self.original_config
    }

    fn reset_to_original(&mut self) {
        self.config = self.original_config.clone();
        self.fields_per_tab = Self::build_fields(&self.config);
    }

    fn save_config(&mut self) -> Result<(), anyhow::Error> {
        if let Some(ref path) = self.config_path {
            self.config.save(path)?;
        }
        self.original_config = self.config.clone();
        self.save_flash = Some(std::time::Instant::now());
        Ok(())
    }

    fn build_fields(config: &Config) -> Vec<Vec<SettingsField>> {
        vec![
            Self::build_project_fields(config),
            Self::build_sessions_fields(config),
            Self::build_budget_fields(config),
            Self::build_github_fields(config),
            Self::build_notifications_fields(config),
            Self::build_gates_fields(config),
            Self::build_review_fields(config),
            Self::build_theme_fields(config),
            Self::build_layout_fields(config),
            Self::build_advanced_fields(config),
        ]
    }

    fn field(widget: WidgetKind) -> SettingsField {
        SettingsField { widget }
    }

    fn build_project_fields(config: &Config) -> Vec<SettingsField> {
        vec![
            Self::field(WidgetKind::TextInput(TextInput::new(
                "repo",
                &config.project.repo,
            ))),
            Self::field(WidgetKind::TextInput(TextInput::new(
                "base_branch",
                &config.project.base_branch,
            ))),
        ]
    }

    fn build_sessions_fields(config: &Config) -> Vec<SettingsField> {
        let s = &config.sessions;
        let permission_options: Vec<String> = vec![
            "default",
            "acceptEdits",
            "bypassPermissions",
            "dontAsk",
            "plan",
            "auto",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let perm_idx = permission_options
            .iter()
            .position(|p| p == &s.permission_mode)
            .unwrap_or(0);

        vec![
            Self::field(WidgetKind::NumberStepper(NumberStepper::new(
                "max_concurrent",
                s.max_concurrent as i64,
                1,
                20,
            ))),
            Self::field(WidgetKind::NumberStepper(
                NumberStepper::new("stall_timeout_secs", s.stall_timeout_secs as i64, 30, 3600)
                    .with_step(30),
            )),
            Self::field(WidgetKind::TextInput(TextInput::new(
                "default_model",
                &s.default_model,
            ))),
            Self::field(WidgetKind::TextInput(TextInput::new(
                "default_mode",
                &s.default_mode,
            ))),
            Self::field(WidgetKind::Dropdown(Dropdown::new(
                "permission_mode",
                permission_options,
                perm_idx,
            ))),
            Self::field(WidgetKind::NumberStepper(NumberStepper::new(
                "max_retries",
                s.max_retries as i64,
                0,
                10,
            ))),
            Self::field(WidgetKind::NumberStepper(
                NumberStepper::new("retry_cooldown_secs", s.retry_cooldown_secs as i64, 0, 600)
                    .with_step(10),
            )),
            // Context Overflow sub-section
            Self::field(WidgetKind::NumberStepper(
                NumberStepper::new(
                    "overflow_threshold_pct",
                    s.context_overflow.overflow_threshold_pct as i64,
                    10,
                    100,
                )
                .with_step(5),
            )),
            Self::field(WidgetKind::Toggle(Toggle::new(
                "auto_fork",
                s.context_overflow.auto_fork,
            ))),
            Self::field(WidgetKind::NumberStepper(
                NumberStepper::new(
                    "commit_prompt_pct",
                    s.context_overflow.commit_prompt_pct as i64,
                    10,
                    100,
                )
                .with_step(5),
            )),
            Self::field(WidgetKind::NumberStepper(NumberStepper::new(
                "max_fork_depth",
                s.context_overflow.max_fork_depth as i64,
                1,
                20,
            ))),
            // Conflict sub-section
            Self::field(WidgetKind::Toggle(Toggle::new(
                "conflict_enabled",
                s.conflict.enabled,
            ))),
            Self::field(WidgetKind::Dropdown(Dropdown::new(
                "conflict_policy",
                vec!["warn".into(), "pause".into(), "kill".into()],
                match s.conflict.policy {
                    crate::config::ConflictPolicy::Warn => 0,
                    crate::config::ConflictPolicy::Pause => 1,
                    crate::config::ConflictPolicy::Kill => 2,
                },
            ))),
        ]
    }

    fn build_budget_fields(config: &Config) -> Vec<SettingsField> {
        let b = &config.budget;
        vec![
            Self::field(WidgetKind::NumberStepper(
                NumberStepper::new(
                    "per_session_usd",
                    (b.per_session_usd * 10.0) as i64,
                    1,
                    1000,
                )
                .with_step(5),
            )),
            Self::field(WidgetKind::NumberStepper(
                NumberStepper::new("total_usd", (b.total_usd * 10.0) as i64, 1, 10000)
                    .with_step(50),
            )),
            Self::field(WidgetKind::NumberStepper(
                NumberStepper::new("alert_threshold_pct", b.alert_threshold_pct as i64, 10, 100)
                    .with_step(5),
            )),
        ]
    }

    fn build_github_fields(config: &Config) -> Vec<SettingsField> {
        let g = &config.github;
        let merge_options: Vec<String> = vec!["merge", "squash", "rebase"]
            .into_iter()
            .map(String::from)
            .collect();
        let merge_idx = match g.merge_method {
            crate::config::MergeMethod::Merge => 0,
            crate::config::MergeMethod::Squash => 1,
            crate::config::MergeMethod::Rebase => 2,
        };
        vec![
            Self::field(WidgetKind::ListEditor(ListEditor::new(
                "issue_filter_labels",
                g.issue_filter_labels.clone(),
            ))),
            Self::field(WidgetKind::Toggle(Toggle::new("auto_pr", g.auto_pr))),
            Self::field(WidgetKind::NumberStepper(
                NumberStepper::new("cache_ttl_secs", g.cache_ttl_secs as i64, 30, 3600)
                    .with_step(30),
            )),
            Self::field(WidgetKind::Toggle(Toggle::new("auto_merge", g.auto_merge))),
            Self::field(WidgetKind::Dropdown(Dropdown::new(
                "merge_method",
                merge_options,
                merge_idx,
            ))),
        ]
    }

    fn build_notifications_fields(config: &Config) -> Vec<SettingsField> {
        let n = &config.notifications;
        vec![
            Self::field(WidgetKind::Toggle(Toggle::new("desktop", n.desktop))),
            Self::field(WidgetKind::Toggle(Toggle::new("slack", n.slack))),
            Self::field(WidgetKind::TextInput(TextInput::new(
                "slack_webhook_url",
                n.slack_webhook_url.as_deref().unwrap_or(""),
            ))),
            Self::field(WidgetKind::NumberStepper(NumberStepper::new(
                "slack_rate_limit_per_min",
                n.slack_rate_limit_per_min as i64,
                1,
                60,
            ))),
        ]
    }

    fn build_gates_fields(config: &Config) -> Vec<SettingsField> {
        let g = &config.gates;
        vec![
            Self::field(WidgetKind::Toggle(Toggle::new("enabled", g.enabled))),
            Self::field(WidgetKind::TextInput(TextInput::new(
                "test_command",
                &g.test_command,
            ))),
            Self::field(WidgetKind::NumberStepper(
                NumberStepper::new(
                    "ci_poll_interval_secs",
                    g.ci_poll_interval_secs as i64,
                    5,
                    300,
                )
                .with_step(5),
            )),
            Self::field(WidgetKind::NumberStepper(
                NumberStepper::new("ci_max_wait_secs", g.ci_max_wait_secs as i64, 60, 7200)
                    .with_step(60),
            )),
            Self::field(WidgetKind::Toggle(Toggle::new(
                "ci_auto_fix.enabled",
                g.ci_auto_fix.enabled,
            ))),
            Self::field(WidgetKind::NumberStepper(NumberStepper::new(
                "ci_auto_fix.max_retries",
                g.ci_auto_fix.max_retries as i64,
                0,
                10,
            ))),
        ]
    }

    fn build_review_fields(config: &Config) -> Vec<SettingsField> {
        let r = &config.review;
        vec![
            Self::field(WidgetKind::Toggle(Toggle::new("enabled", r.enabled))),
            Self::field(WidgetKind::TextInput(TextInput::new("command", &r.command))),
        ]
    }

    fn build_theme_fields(config: &Config) -> Vec<SettingsField> {
        use crate::tui::theme::ThemePreset;
        let preset_options: Vec<String> = vec!["dark".into(), "light".into()];
        let preset_idx = match config.tui.theme.preset {
            ThemePreset::Dark => 0,
            ThemePreset::Light => 1,
        };
        vec![
            Self::field(WidgetKind::Toggle(Toggle::new("live_preview", false))),
            Self::field(WidgetKind::Dropdown(Dropdown::new(
                "preset",
                preset_options,
                preset_idx,
            ))),
        ]
    }

    fn build_layout_fields(config: &Config) -> Vec<SettingsField> {
        use crate::config::{Density, LayoutMode};
        let mode_options: Vec<String> = vec!["vertical".into(), "horizontal".into()];
        let mode_idx = match config.tui.layout.mode {
            LayoutMode::Vertical => 0,
            LayoutMode::Horizontal => 1,
        };
        let density_options: Vec<String> =
            vec!["default".into(), "comfortable".into(), "compact".into()];
        let density_idx = match config.tui.layout.density {
            Density::Default => 0,
            Density::Comfortable => 1,
            Density::Compact => 2,
        };
        vec![
            Self::field(WidgetKind::Dropdown(Dropdown::new(
                "mode",
                mode_options,
                mode_idx,
            ))),
            Self::field(WidgetKind::Dropdown(Dropdown::new(
                "density",
                density_options,
                density_idx,
            ))),
            Self::field(WidgetKind::NumberStepper(
                NumberStepper::new(
                    "preview_ratio",
                    config.tui.layout.preview_ratio as i64,
                    10,
                    90,
                )
                .with_step(5),
            )),
            Self::field(WidgetKind::NumberStepper(
                NumberStepper::new(
                    "activity_log_height",
                    config.tui.layout.activity_log_height as i64,
                    10,
                    50,
                )
                .with_step(5),
            )),
        ]
    }

    fn build_advanced_fields(config: &Config) -> Vec<SettingsField> {
        vec![
            Self::field(WidgetKind::NumberStepper(NumberStepper::new(
                "heavy_task_limit",
                config.concurrency.heavy_task_limit as i64,
                1,
                10,
            ))),
            Self::field(WidgetKind::NumberStepper(
                NumberStepper::new(
                    "work_tick_interval_secs",
                    config.monitoring.work_tick_interval_secs as i64,
                    1,
                    120,
                )
                .with_step(5),
            )),
            Self::field(WidgetKind::ListEditor(ListEditor::new(
                "heavy_task_labels",
                config.concurrency.heavy_task_labels.clone(),
            ))),
        ]
    }

    pub fn active_tab(&self) -> SettingsTab {
        SettingsTab::ALL[self.active_tab]
    }

    fn current_fields(&self) -> &[SettingsField] {
        &self.fields_per_tab[self.active_tab]
    }

    fn current_fields_mut(&mut self) -> &mut [SettingsField] {
        &mut self.fields_per_tab[self.active_tab]
    }

    fn field_count(&self) -> usize {
        self.current_fields().len()
    }

    fn next_tab(&mut self) {
        self.active_tab = (self.active_tab + 1) % SettingsTab::ALL.len();
        self.field_index = 0;
        self.scroll_offset = 0;
    }

    fn prev_tab(&mut self) {
        self.active_tab = if self.active_tab == 0 {
            SettingsTab::ALL.len() - 1
        } else {
            self.active_tab - 1
        };
        self.field_index = 0;
        self.scroll_offset = 0;
    }

    fn active_widget_needs_insert(&self) -> bool {
        self.current_fields()
            .get(self.field_index)
            .is_some_and(|f| f.widget.needs_insert_mode())
    }

    /// Synchronize widget values back into the Config struct.
    pub fn sync_widgets_to_config(&mut self) {
        // Project (tab 0)
        if let Some(fields) = self.fields_per_tab.first() {
            if let Some(WidgetKind::TextInput(w)) = fields.first().map(|f| &f.widget) {
                self.config.project.repo = w.value.clone();
            }
            if let Some(WidgetKind::TextInput(w)) = fields.get(1).map(|f| &f.widget) {
                self.config.project.base_branch = w.value.clone();
            }
        }

        // Sessions (tab 1)
        if let Some(fields) = self.fields_per_tab.get(1) {
            let s = &mut self.config.sessions;
            if let Some(WidgetKind::NumberStepper(w)) = fields.first().map(|f| &f.widget) {
                s.max_concurrent = w.value as usize;
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(1).map(|f| &f.widget) {
                s.stall_timeout_secs = w.value as u64;
            }
            if let Some(WidgetKind::TextInput(w)) = fields.get(2).map(|f| &f.widget) {
                s.default_model = w.value.clone();
            }
            if let Some(WidgetKind::TextInput(w)) = fields.get(3).map(|f| &f.widget) {
                s.default_mode = w.value.clone();
            }
            if let Some(WidgetKind::Dropdown(w)) = fields.get(4).map(|f| &f.widget) {
                s.permission_mode = w.selected_value().to_string();
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(5).map(|f| &f.widget) {
                s.max_retries = w.value as u32;
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(6).map(|f| &f.widget) {
                s.retry_cooldown_secs = w.value as u64;
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(7).map(|f| &f.widget) {
                s.context_overflow.overflow_threshold_pct = w.value as u8;
            }
            if let Some(WidgetKind::Toggle(w)) = fields.get(8).map(|f| &f.widget) {
                s.context_overflow.auto_fork = w.value;
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(9).map(|f| &f.widget) {
                s.context_overflow.commit_prompt_pct = w.value as u8;
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(10).map(|f| &f.widget) {
                s.context_overflow.max_fork_depth = w.value as u8;
            }
            if let Some(WidgetKind::Toggle(w)) = fields.get(11).map(|f| &f.widget) {
                s.conflict.enabled = w.value;
            }
            if let Some(WidgetKind::Dropdown(w)) = fields.get(12).map(|f| &f.widget) {
                s.conflict.policy = match w.selected {
                    0 => crate::config::ConflictPolicy::Warn,
                    1 => crate::config::ConflictPolicy::Pause,
                    _ => crate::config::ConflictPolicy::Kill,
                };
            }
        }

        // Budget (tab 2) — values stored as x10 for decimal precision
        if let Some(fields) = self.fields_per_tab.get(2) {
            if let Some(WidgetKind::NumberStepper(w)) = fields.first().map(|f| &f.widget) {
                self.config.budget.per_session_usd = w.value as f64 / 10.0;
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(1).map(|f| &f.widget) {
                self.config.budget.total_usd = w.value as f64 / 10.0;
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(2).map(|f| &f.widget) {
                self.config.budget.alert_threshold_pct = w.value as u8;
            }
        }

        // GitHub (tab 3)
        if let Some(fields) = self.fields_per_tab.get(3) {
            let g = &mut self.config.github;
            if let Some(WidgetKind::ListEditor(w)) = fields.first().map(|f| &f.widget) {
                g.issue_filter_labels = w.items.clone();
            }
            if let Some(WidgetKind::Toggle(w)) = fields.get(1).map(|f| &f.widget) {
                g.auto_pr = w.value;
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(2).map(|f| &f.widget) {
                g.cache_ttl_secs = w.value as u64;
            }
            if let Some(WidgetKind::Toggle(w)) = fields.get(3).map(|f| &f.widget) {
                g.auto_merge = w.value;
            }
            if let Some(WidgetKind::Dropdown(w)) = fields.get(4).map(|f| &f.widget) {
                g.merge_method = match w.selected {
                    0 => crate::config::MergeMethod::Merge,
                    1 => crate::config::MergeMethod::Squash,
                    _ => crate::config::MergeMethod::Rebase,
                };
            }
        }

        // Notifications (tab 4)
        if let Some(fields) = self.fields_per_tab.get(4) {
            let n = &mut self.config.notifications;
            if let Some(WidgetKind::Toggle(w)) = fields.first().map(|f| &f.widget) {
                n.desktop = w.value;
            }
            if let Some(WidgetKind::Toggle(w)) = fields.get(1).map(|f| &f.widget) {
                n.slack = w.value;
            }
            if let Some(WidgetKind::TextInput(w)) = fields.get(2).map(|f| &f.widget) {
                n.slack_webhook_url = if w.value.is_empty() {
                    None
                } else {
                    Some(w.value.clone())
                };
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(3).map(|f| &f.widget) {
                n.slack_rate_limit_per_min = w.value as u32;
            }
        }

        // Gates (tab 5)
        if let Some(fields) = self.fields_per_tab.get(5) {
            let g = &mut self.config.gates;
            if let Some(WidgetKind::Toggle(w)) = fields.first().map(|f| &f.widget) {
                g.enabled = w.value;
            }
            if let Some(WidgetKind::TextInput(w)) = fields.get(1).map(|f| &f.widget) {
                g.test_command = w.value.clone();
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(2).map(|f| &f.widget) {
                g.ci_poll_interval_secs = w.value as u64;
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(3).map(|f| &f.widget) {
                g.ci_max_wait_secs = w.value as u64;
            }
            if let Some(WidgetKind::Toggle(w)) = fields.get(4).map(|f| &f.widget) {
                g.ci_auto_fix.enabled = w.value;
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(5).map(|f| &f.widget) {
                g.ci_auto_fix.max_retries = w.value as u32;
            }
        }

        // Review (tab 6)
        if let Some(fields) = self.fields_per_tab.get(6) {
            let r = &mut self.config.review;
            if let Some(WidgetKind::Toggle(w)) = fields.first().map(|f| &f.widget) {
                r.enabled = w.value;
            }
            if let Some(WidgetKind::TextInput(w)) = fields.get(1).map(|f| &f.widget) {
                r.command = w.value.clone();
            }
        }

        // Theme (tab 7)
        if let Some(fields) = self.fields_per_tab.get(7) {
            if let Some(WidgetKind::Toggle(w)) = fields.first().map(|f| &f.widget) {
                self.live_preview = w.value;
            }
            if let Some(WidgetKind::Dropdown(w)) = fields.get(1).map(|f| &f.widget) {
                self.config.tui.theme.preset = match w.selected {
                    0 => crate::tui::theme::ThemePreset::Dark,
                    _ => crate::tui::theme::ThemePreset::Light,
                };
            }
        }

        // Layout (tab 8)
        if let Some(fields) = self.fields_per_tab.get(8) {
            let l = &mut self.config.tui.layout;
            if let Some(WidgetKind::Dropdown(w)) = fields.first().map(|f| &f.widget) {
                l.mode = match w.selected {
                    0 => crate::config::LayoutMode::Vertical,
                    _ => crate::config::LayoutMode::Horizontal,
                };
            }
            if let Some(WidgetKind::Dropdown(w)) = fields.get(1).map(|f| &f.widget) {
                l.density = match w.selected {
                    0 => crate::config::Density::Default,
                    1 => crate::config::Density::Comfortable,
                    _ => crate::config::Density::Compact,
                };
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(2).map(|f| &f.widget) {
                l.preview_ratio = w.value as u8;
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(3).map(|f| &f.widget) {
                l.activity_log_height = w.value as u8;
            }
        }

        // Advanced (tab 9)
        if let Some(fields) = self.fields_per_tab.get(9) {
            if let Some(WidgetKind::NumberStepper(w)) = fields.first().map(|f| &f.widget) {
                self.config.concurrency.heavy_task_limit = w.value as usize;
            }
            if let Some(WidgetKind::NumberStepper(w)) = fields.get(1).map(|f| &f.widget) {
                self.config.monitoring.work_tick_interval_secs = w.value as u64;
            }
            if let Some(WidgetKind::ListEditor(w)) = fields.get(2).map(|f| &f.widget) {
                self.config.concurrency.heavy_task_labels = w.items.clone();
            }
        }
    }

    fn draw_tab_bar(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut spans = Vec::new();
        for (i, tab) in SettingsTab::ALL.iter().enumerate() {
            let style = if i == self.active_tab {
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(theme.text_secondary)
            };
            if i > 0 {
                spans.push(Span::styled(
                    " │ ",
                    Style::default().fg(theme.border_inactive),
                ));
            }
            spans.push(Span::styled(tab.label(), style));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn draw_fields(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let visible_rows = area.height as usize;
        let field_count = self.field_count();
        let field_index = self.field_index;
        let tab = self.active_tab;

        // Adjust scroll so focused field is visible
        if field_index < self.scroll_offset {
            self.scroll_offset = field_index;
        } else if field_index >= self.scroll_offset + visible_rows {
            self.scroll_offset = field_index.saturating_sub(visible_rows - 1);
        }

        let scroll_offset = self.scroll_offset;
        let fields = &self.fields_per_tab[tab];
        for (draw_row, field_idx) in (scroll_offset..).take(visible_rows).enumerate() {
            if field_idx >= field_count {
                break;
            }
            let focused = field_idx == field_index;
            let field_area = Rect {
                x: area.x,
                y: area.y + draw_row as u16,
                width: area.width,
                height: 1,
            };
            fields[field_idx].widget.draw(f, field_area, theme, focused);
        }
    }
}

impl Screen for SettingsScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        let Event::Key(KeyEvent {
            code,
            modifiers,
            kind: crossterm::event::KeyEventKind::Press,
            ..
        }) = event
        else {
            return ScreenAction::None;
        };

        if self.active_widget_needs_insert() {
            let idx = self.field_index;
            let tab = self.active_tab;
            let key_event = KeyEvent::new(*code, *modifiers);
            if let Some(field) = self.fields_per_tab[tab].get_mut(idx) {
                field.widget.handle_input(key_event);
            }
            self.sync_widgets_to_config();
            return ScreenAction::None;
        }

        // Handle discard confirmation
        if self.confirm_discard {
            return match *code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.confirm_discard = false;
                    // Clear preview on discard — the Pop handler will also clear it
                    ScreenAction::Pop
                }
                _ => {
                    self.confirm_discard = false;
                    ScreenAction::None
                }
            };
        }

        match (*code, *modifiers) {
            (KeyCode::Esc, _) => {
                if self.is_dirty() {
                    self.confirm_discard = true;
                    ScreenAction::None
                } else {
                    ScreenAction::Pop
                }
            }
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                let _ = self.save_config();
                let config = self.config.clone();
                // Promote preview to actual theme on save
                self.live_preview = false;
                ScreenAction::UpdateConfig(Box::new(config))
            }
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                self.reset_to_original();
                self.live_preview = false;
                ScreenAction::PreviewTheme(None)
            }
            (KeyCode::Tab, _) => {
                self.next_tab();
                ScreenAction::None
            }
            (KeyCode::BackTab, _) => {
                self.prev_tab();
                ScreenAction::None
            }
            (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
                self.field_index = self.field_index.saturating_sub(1);
                ScreenAction::None
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
                if self.field_count() > 0 && self.field_index + 1 < self.field_count() {
                    self.field_index += 1;
                }
                ScreenAction::None
            }
            _ => {
                // Delegate to active widget for non-navigation keys
                let idx = self.field_index;
                let tab = self.active_tab;
                let key_event = KeyEvent::new(*code, *modifiers);
                let changed = self.fields_per_tab[tab]
                    .get_mut(idx)
                    .map(|f| f.widget.handle_input(key_event))
                    == Some(WidgetAction::Changed);
                if changed {
                    self.sync_widgets_to_config();
                    if self.live_preview {
                        return ScreenAction::PreviewTheme(Some(self.config.tui.theme.clone()));
                    }
                }
                ScreenAction::None
            }
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let title = if self.is_dirty() {
            " Settings [Modified] "
        } else if self.save_flash.is_some_and(|t| t.elapsed().as_secs() < 2) {
            " Settings [Saved] "
        } else {
            " Settings "
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent_success))
            .title(Span::styled(
                title,
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        f.render_widget(block, area);

        if inner.height < 4 || inner.width < 20 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // tab bar
                Constraint::Length(1), // separator
                Constraint::Min(1),    // field list
                Constraint::Length(1), // keybinds
            ])
            .split(inner);

        self.draw_tab_bar(f, chunks[0], theme);

        let sep = "─".repeat(inner.width as usize);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                sep,
                Style::default().fg(theme.border_inactive),
            ))),
            chunks[1],
        );

        self.draw_fields(f, chunks[2], theme);

        if self.confirm_discard {
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        "Unsaved changes. Discard? ",
                        Style::default()
                            .fg(theme.accent_warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("(y/n)", Style::default().fg(theme.text_secondary)),
                ])),
                chunks[3],
            );
        } else {
            draw_keybinds_bar(
                f,
                chunks[3],
                &[
                    ("Tab", "Tab"),
                    ("↑/↓", "Field"),
                    ("Esc", "Back"),
                    ("Ctrl+s", "Save"),
                ],
                theme,
            );
        }
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        if self.active_widget_needs_insert() {
            Some(InputMode::Insert)
        } else {
            Some(InputMode::Normal)
        }
    }
}

impl KeymapProvider for SettingsScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![
            KeyBindingGroup {
                title: "Navigation",
                bindings: vec![
                    KeyBinding {
                        key: "Tab/Shift+Tab",
                        description: "Switch tab",
                    },
                    KeyBinding {
                        key: "↑/↓ or j/k",
                        description: "Navigate fields",
                    },
                    KeyBinding {
                        key: "Esc",
                        description: "Back to Dashboard",
                    },
                ],
            },
            KeyBindingGroup {
                title: "Actions",
                bindings: vec![
                    KeyBinding {
                        key: "Ctrl+s",
                        description: "Save changes",
                    },
                    KeyBinding {
                        key: "Ctrl+r",
                        description: "Reset all fields",
                    },
                ],
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::key_event;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn make_config() -> Config {
        let toml_str = r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        use std::io::Write;
        write!(f, "{}", toml_str).unwrap();
        Config::load(f.path()).unwrap()
    }

    #[test]
    fn initial_tab_is_project() {
        let screen = SettingsScreen::new(make_config());
        assert_eq!(screen.active_tab(), SettingsTab::Project);
    }

    #[test]
    fn tab_cycles_right() {
        let mut screen = SettingsScreen::new(make_config());
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        assert_eq!(screen.active_tab(), SettingsTab::Sessions);
    }

    #[test]
    fn tab_wraps_right() {
        let mut screen = SettingsScreen::new(make_config());
        for _ in 0..SettingsTab::ALL.len() {
            screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        }
        assert_eq!(screen.active_tab(), SettingsTab::Project);
    }

    #[test]
    fn tab_wraps_left() {
        let mut screen = SettingsScreen::new(make_config());
        screen.handle_input(&key_event(KeyCode::BackTab), InputMode::Normal);
        assert_eq!(screen.active_tab(), SettingsTab::Advanced);
    }

    #[test]
    fn field_navigation() {
        let mut screen = SettingsScreen::new(make_config());
        assert_eq!(screen.field_index, 0);
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        assert_eq!(screen.field_index, 1);
        screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
        assert_eq!(screen.field_index, 0);
    }

    #[test]
    fn esc_returns_pop() {
        let mut screen = SettingsScreen::new(make_config());
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn tab_switch_resets_field_index() {
        let mut screen = SettingsScreen::new(make_config());
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        assert!(screen.field_index > 0);
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        assert_eq!(screen.field_index, 0);
    }

    #[test]
    fn toggle_widget_changes_config() {
        let mut screen = SettingsScreen::new(make_config());
        // Navigate to Notifications tab (index 4)
        for _ in 0..4 {
            screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        }
        assert_eq!(screen.active_tab(), SettingsTab::Notifications);
        // First field is "desktop" (Toggle, default true)
        assert!(screen.config.notifications.desktop);
        // Toggle it
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        assert!(!screen.config.notifications.desktop);
    }

    #[test]
    fn number_stepper_changes_config() {
        let mut screen = SettingsScreen::new(make_config());
        // Navigate to Sessions tab
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        assert_eq!(screen.active_tab(), SettingsTab::Sessions);
        // First field is max_concurrent (NumberStepper, default 3)
        let orig = screen.config.sessions.max_concurrent;
        // Increment
        screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal);
        assert_eq!(screen.config.sessions.max_concurrent, orig + 1);
    }

    #[test]
    fn dropdown_cycles_config() {
        let mut screen = SettingsScreen::new(make_config());
        // Navigate to GitHub tab (index 3)
        for _ in 0..3 {
            screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        }
        // Navigate to merge_method (last field, index 4)
        for _ in 0..4 {
            screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        }
        // Default is squash (index 1), cycle right to rebase (index 2)
        screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal);
        assert_eq!(
            screen.config.github.merge_method,
            crate::config::MergeMethod::Rebase
        );
    }

    #[test]
    fn desired_input_mode_normal_by_default() {
        let screen = SettingsScreen::new(make_config());
        assert_eq!(screen.desired_input_mode(), Some(InputMode::Normal));
    }

    #[test]
    fn keybindings_returns_non_empty() {
        let screen = SettingsScreen::new(make_config());
        let groups = screen.keybindings();
        assert!(!groups.is_empty());
    }

    #[test]
    fn all_tabs_have_fields() {
        let screen = SettingsScreen::new(make_config());
        for (i, tab) in SettingsTab::ALL.iter().enumerate() {
            assert!(
                !screen.fields_per_tab[i].is_empty(),
                "Tab {:?} has no fields",
                tab
            );
        }
    }

    // --- Issue #74: Dirty state tests ---

    #[test]
    fn initially_not_dirty() {
        let screen = SettingsScreen::new(make_config());
        assert!(!screen.is_dirty());
    }

    #[test]
    fn modify_makes_dirty() {
        let mut screen = SettingsScreen::new(make_config());
        // Toggle desktop notification
        for _ in 0..4 {
            screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        }
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        assert!(screen.is_dirty());
    }

    #[test]
    fn ctrl_r_resets_dirty() {
        let mut screen = SettingsScreen::new(make_config());
        let orig_desktop = screen.config.notifications.desktop;
        // Modify
        for _ in 0..4 {
            screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        }
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        assert!(screen.is_dirty());
        // Reset
        let ctrl_r = Event::Key(KeyEvent {
            code: KeyCode::Char('r'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        screen.handle_input(&ctrl_r, InputMode::Normal);
        assert!(!screen.is_dirty());
        assert_eq!(screen.config.notifications.desktop, orig_desktop);
    }

    #[test]
    fn esc_with_dirty_shows_confirmation() {
        let mut screen = SettingsScreen::new(make_config());
        // Modify
        for _ in 0..4 {
            screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        }
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        assert!(screen.is_dirty());
        // Esc should trigger confirmation, not pop
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert!(screen.confirm_discard);
    }

    #[test]
    fn confirm_discard_y_pops() {
        let mut screen = SettingsScreen::new(make_config());
        screen.confirm_discard = true;
        let action = screen.handle_input(&key_event(KeyCode::Char('y')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn confirm_discard_n_cancels() {
        let mut screen = SettingsScreen::new(make_config());
        screen.confirm_discard = true;
        let action = screen.handle_input(&key_event(KeyCode::Char('n')), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert!(!screen.confirm_discard);
    }

    #[test]
    fn ctrl_s_saves_and_returns_update_config() {
        let mut screen = SettingsScreen::new(make_config());
        // Modify
        for _ in 0..4 {
            screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        }
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        assert!(screen.is_dirty());
        // Save
        let ctrl_s = Event::Key(KeyEvent {
            code: KeyCode::Char('s'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let action = screen.handle_input(&ctrl_s, InputMode::Normal);
        assert!(!screen.is_dirty()); // original updated
        assert!(matches!(action, ScreenAction::UpdateConfig(_)));
    }

    #[test]
    fn ctrl_s_writes_to_file() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
        )
        .unwrap();
        let config = Config::load(f.path()).unwrap();
        let mut screen = SettingsScreen::new(config).with_config_path(f.path().to_path_buf());
        // Modify desktop notifications
        for _ in 0..4 {
            screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        }
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        // Save
        let ctrl_s = Event::Key(KeyEvent {
            code: KeyCode::Char('s'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        screen.handle_input(&ctrl_s, InputMode::Normal);
        // Reload and verify
        let reloaded = Config::load(f.path()).unwrap();
        assert!(!reloaded.notifications.desktop);
    }

    // --- Issue #77: Integration tests ---

    #[test]
    fn integration_full_settings_flow_modify_save_reload() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
max_concurrent = 3
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
auto_pr = true
[notifications]
desktop = true
"#
        )
        .unwrap();
        let config = Config::load(f.path()).unwrap();
        let mut screen = SettingsScreen::new(config).with_config_path(f.path().to_path_buf());

        // Modify: sessions tab, increment max_concurrent
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal); // 3 -> 4
        assert_eq!(screen.config.sessions.max_concurrent, 4);
        assert!(screen.is_dirty());

        // Save
        let ctrl_s = Event::Key(KeyEvent {
            code: KeyCode::Char('s'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let action = screen.handle_input(&ctrl_s, InputMode::Normal);
        assert!(!screen.is_dirty());
        assert!(matches!(action, ScreenAction::UpdateConfig(_)));

        // Reload file and verify
        let reloaded = Config::load(f.path()).unwrap();
        assert_eq!(reloaded.sessions.max_concurrent, 4);
    }

    #[test]
    fn integration_modify_esc_confirm_discard_verify_file_unchanged() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"
[project]
repo = "owner/repo"
[sessions]
max_concurrent = 3
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
        )
        .unwrap();
        let config = Config::load(f.path()).unwrap();
        let mut screen = SettingsScreen::new(config).with_config_path(f.path().to_path_buf());

        // Modify
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal);
        assert!(screen.is_dirty());

        // Esc triggers confirmation
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert!(screen.confirm_discard);

        // Confirm discard
        let action = screen.handle_input(&key_event(KeyCode::Char('y')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);

        // File should be unchanged
        let reloaded = Config::load(f.path()).unwrap();
        assert_eq!(reloaded.sessions.max_concurrent, 3);
    }

    #[test]
    fn integration_modify_ctrl_r_verify_all_fields_reset() {
        let mut screen = SettingsScreen::new(make_config());
        let orig = screen.config.clone();

        // Modify multiple things
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal); // Sessions
        screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal); // max_concurrent++

        for _ in 0..3 {
            screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        } // Notifications
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal); // toggle desktop

        assert!(screen.is_dirty());

        // Reset
        let ctrl_r = Event::Key(KeyEvent {
            code: KeyCode::Char('r'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        screen.handle_input(&ctrl_r, InputMode::Normal);
        assert!(!screen.is_dirty());
        assert_eq!(screen.config, orig);
    }

    #[test]
    fn integration_theme_preview_on_change_emits_preview() {
        let mut screen = SettingsScreen::new(make_config());

        // Go to Theme tab (index 7)
        for _ in 0..7 {
            screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        }
        assert_eq!(screen.active_tab(), SettingsTab::Theme);

        // First field is live_preview toggle (default off)
        assert!(!screen.live_preview);
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        assert!(screen.live_preview);

        // Move to preset dropdown
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        // Change preset — should emit PreviewTheme
        let action = screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal);
        assert!(matches!(action, ScreenAction::PreviewTheme(Some(_))));
    }

    #[test]
    fn integration_theme_preview_reset_clears_preview() {
        let mut screen = SettingsScreen::new(make_config());
        screen.live_preview = true;

        let ctrl_r = Event::Key(KeyEvent {
            code: KeyCode::Char('r'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let action = screen.handle_input(&ctrl_r, InputMode::Normal);
        assert!(matches!(action, ScreenAction::PreviewTheme(None)));
        assert!(!screen.live_preview);
    }

    #[test]
    fn integration_layout_tab_fields() {
        let mut screen = SettingsScreen::new(make_config());
        // Navigate to Layout tab (index 8)
        for _ in 0..8 {
            screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        }
        assert_eq!(screen.active_tab(), SettingsTab::Layout);
        assert_eq!(screen.field_count(), 4); // mode, density, preview_ratio, activity_log_height

        // Cycle mode from vertical to horizontal
        screen.handle_input(&key_event(KeyCode::Char('l')), InputMode::Normal);
        assert_eq!(
            screen.config.tui.layout.mode,
            crate::config::LayoutMode::Horizontal
        );
    }

    #[test]
    fn integration_keybindings_grouped_logically() {
        let screen = SettingsScreen::new(make_config());
        let groups = screen.keybindings();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].title, "Navigation");
        assert_eq!(groups[1].title, "Actions");
        assert!(groups[0].bindings.len() >= 3);
        assert!(groups[1].bindings.len() >= 2);
    }
}
