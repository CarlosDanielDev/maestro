mod draw;
pub mod types;

pub use types::*;

use crate::adapt::types::{AdaptPlan, AdaptReport, MaterializeResult, ProjectProfile};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{Frame, layout::Rect};

use super::{Screen, ScreenAction};

const FIELD_COUNT: usize = 5;

pub struct AdaptScreen {
    pub step: AdaptStep,
    pub config: AdaptWizardConfig,
    pub selected_field: usize,
    pub results: AdaptResults,
    pub error: Option<AdaptError>,
    pub spinner_tick: usize,
    pub scroll_offset: u16,
    pub cancelled: bool,
    /// Whether a cache from a previous incomplete run was loaded.
    pub loaded_from_cache: bool,
}

impl AdaptScreen {
    pub fn new() -> Self {
        let (results, loaded_from_cache) = match AdaptResults::load_cache() {
            Some(cached) => (cached, true),
            None => (AdaptResults::default(), false),
        };
        Self {
            step: AdaptStep::Configure,
            config: AdaptWizardConfig::default(),
            selected_field: 0,
            results,
            error: None,
            spinner_tick: 0,
            scroll_offset: 0,
            cancelled: false,
            loaded_from_cache,
        }
    }

    pub fn tick(&mut self) {
        self.spinner_tick = self.spinner_tick.wrapping_add(1);
    }

    pub fn set_scan_result(&mut self, profile: ProjectProfile) {
        self.results.profile = Some(profile);
    }

    pub fn set_analyze_result(&mut self, report: AdaptReport) {
        self.results.report = Some(report);
    }

    pub fn set_plan_result(&mut self, plan: AdaptPlan) {
        self.results.plan = Some(plan);
    }

    pub fn set_materialize_result(&mut self, result: MaterializeResult) {
        self.results.materialize = Some(result);
    }

    pub fn set_error(&mut self, phase: AdaptStep, message: String) {
        self.error = Some(AdaptError { phase, message });
        self.step = AdaptStep::Failed;
    }

    pub fn build_adapt_config(&self) -> crate::adapt::AdaptConfig {
        self.config.to_adapt_config()
    }

    /// Advance the wizard after a successful scan phase.
    /// Returns the next TuiCommand to queue, or None if pipeline is complete.
    pub fn complete_scan(
        &mut self,
        profile: ProjectProfile,
    ) -> Option<crate::tui::app::TuiCommand> {
        self.set_scan_result(profile.clone());
        self.results.save_cache();
        let config = self.build_adapt_config();
        if config.scan_only {
            self.step = AdaptStep::Complete;
            AdaptResults::clear_cache();
            None
        } else {
            self.step = AdaptStep::Analyzing;
            Some(crate::tui::app::TuiCommand::RunAdaptAnalyze(
                config, profile,
            ))
        }
    }

    /// Advance the wizard after a successful analyze phase.
    pub fn complete_analyze(&mut self, report: AdaptReport) -> Option<crate::tui::app::TuiCommand> {
        self.set_analyze_result(report.clone());
        self.results.save_cache();
        let config = self.build_adapt_config();
        if config.no_issues {
            self.step = AdaptStep::Complete;
            AdaptResults::clear_cache();
            None
        } else {
            self.step = AdaptStep::Consolidating;
            let profile = self.results.profile.clone()?;
            Some(crate::tui::app::TuiCommand::RunAdaptConsolidate(
                config, profile, report,
            ))
        }
    }

    pub fn set_prd_result(&mut self, content: String) {
        self.results.prd_content = Some(content);
    }

    /// Advance the wizard after a successful consolidate (PRD) phase.
    pub fn complete_consolidate(
        &mut self,
        prd_content: String,
    ) -> Option<crate::tui::app::TuiCommand> {
        self.set_prd_result(prd_content.clone());
        self.results.save_cache();
        let config = self.build_adapt_config();
        if config.no_issues {
            self.step = AdaptStep::Complete;
            AdaptResults::clear_cache();
            None
        } else {
            self.step = AdaptStep::Planning;
            let profile = self.results.profile.clone()?;
            let report = self.results.report.clone()?;
            Some(crate::tui::app::TuiCommand::RunAdaptPlan(
                config,
                profile,
                report,
                Some(prd_content),
            ))
        }
    }

    /// Advance the wizard after a successful plan phase.
    pub fn complete_plan(&mut self, plan: AdaptPlan) -> Option<crate::tui::app::TuiCommand> {
        self.set_plan_result(plan.clone());
        self.results.save_cache();
        let config = self.build_adapt_config();
        if config.dry_run {
            self.step = AdaptStep::Complete;
            AdaptResults::clear_cache();
            None
        } else {
            self.step = AdaptStep::Materializing;
            let report = self.results.report.clone()?;
            Some(crate::tui::app::TuiCommand::RunAdaptMaterialize(
                plan, report,
            ))
        }
    }

    /// Advance the wizard after a successful materialize phase.
    pub fn complete_materialize(&mut self, result: MaterializeResult) {
        self.set_materialize_result(result);
        self.step = AdaptStep::Complete;
        AdaptResults::clear_cache();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    fn retry(&mut self) -> Option<ScreenAction> {
        if let Some(ref error) = self.error {
            self.step = error.phase;
            self.error = None;
            Some(ScreenAction::StartAdaptPipeline(self.build_adapt_config()))
        } else {
            None
        }
    }

    fn handle_configure_input(&mut self, code: KeyCode) -> ScreenAction {
        match code {
            KeyCode::Char('j') | KeyCode::Down if self.selected_field < FIELD_COUNT - 1 => {
                self.selected_field += 1;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_field = self.selected_field.saturating_sub(1);
            }
            KeyCode::Char(' ') => {
                // Toggle boolean fields
                match self.selected_field {
                    1 => self.config.dry_run = !self.config.dry_run,
                    2 => self.config.scan_only = !self.config.scan_only,
                    3 => self.config.no_issues = !self.config.no_issues,
                    _ => {}
                }
            }
            KeyCode::Backspace => match self.selected_field {
                0 => {
                    self.config.path.pop();
                }
                4 => {
                    self.config.model.pop();
                }
                _ => {}
            },
            KeyCode::Char(c) if self.selected_field == 0 => {
                self.config.path.push(c);
            }
            KeyCode::Char(c) if self.selected_field == 4 => {
                self.config.model.push(c);
            }
            KeyCode::Enter => {
                if self.loaded_from_cache {
                    self.step = self.results.resume_step();
                }
                return ScreenAction::StartAdaptPipeline(self.build_adapt_config());
            }
            KeyCode::Delete | KeyCode::Char('x') if self.loaded_from_cache => {
                self.results = AdaptResults::default();
                AdaptResults::clear_cache();
                self.loaded_from_cache = false;
            }
            KeyCode::Esc => return ScreenAction::Pop,
            _ => {}
        }
        ScreenAction::None
    }
}

impl KeymapProvider for AdaptScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        match self.step {
            AdaptStep::Configure => {
                let mut action_bindings = vec![
                    KeyBinding {
                        key: "Space",
                        description: "Toggle",
                    },
                    KeyBinding {
                        key: "Enter",
                        description: if self.loaded_from_cache {
                            "Resume pipeline"
                        } else {
                            "Start pipeline"
                        },
                    },
                ];
                if self.loaded_from_cache {
                    action_bindings.push(KeyBinding {
                        key: "x",
                        description: "Clear cache (fresh run)",
                    });
                }
                action_bindings.push(KeyBinding {
                    key: "Esc",
                    description: "Back",
                });
                vec![
                    KeyBindingGroup {
                        title: "Navigation",
                        bindings: vec![
                            KeyBinding {
                                key: "j/Down",
                                description: "Move down",
                            },
                            KeyBinding {
                                key: "k/Up",
                                description: "Move up",
                            },
                        ],
                    },
                    KeyBindingGroup {
                        title: "Actions",
                        bindings: action_bindings,
                    },
                ]
            }
            step if step.is_progress() => vec![KeyBindingGroup {
                title: "Actions",
                bindings: vec![KeyBinding {
                    key: "Esc",
                    description: "Cancel",
                }],
            }],
            AdaptStep::Complete => vec![KeyBindingGroup {
                title: "Actions",
                bindings: vec![
                    KeyBinding {
                        key: "j/k",
                        description: "Scroll",
                    },
                    KeyBinding {
                        key: "Esc",
                        description: "Back",
                    },
                ],
            }],
            AdaptStep::Failed => vec![KeyBindingGroup {
                title: "Actions",
                bindings: vec![
                    KeyBinding {
                        key: "r",
                        description: "Retry",
                    },
                    KeyBinding {
                        key: "Esc",
                        description: "Back",
                    },
                ],
            }],
            _ => vec![],
        }
    }
}

impl Screen for AdaptScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            match self.step {
                AdaptStep::Configure => return self.handle_configure_input(*code),
                step if step.is_progress() && *code == KeyCode::Esc => {
                    self.cancelled = true;
                    return ScreenAction::Pop;
                }
                AdaptStep::Complete => match code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        self.scroll_offset = self.scroll_offset.saturating_add(1);
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.scroll_offset = self.scroll_offset.saturating_sub(1);
                    }
                    KeyCode::Esc | KeyCode::Enter => return ScreenAction::Pop,
                    _ => {}
                },
                AdaptStep::Failed => match code {
                    KeyCode::Char('r') => {
                        if let Some(action) = self.retry() {
                            return action;
                        }
                    }
                    KeyCode::Esc => return ScreenAction::Pop,
                    _ => {}
                },
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        draw::draw_adapt_screen(self, f, area, theme);
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        Some(InputMode::Normal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapt::types::*;
    use crate::tui::screens::test_helpers::key_event;

    fn make_mock_profile() -> ProjectProfile {
        ProjectProfile {
            name: "test".into(),
            root: std::path::PathBuf::from("/tmp"),
            language: ProjectLanguage::Rust,
            manifests: vec![],
            config_files: vec![],
            entry_points: vec![],
            source_stats: SourceStats {
                total_files: 10,
                total_lines: 500,
                by_extension: vec![],
            },
            test_infra: TestInfraInfo {
                has_tests: true,
                framework: None,
                test_directories: vec![],
                test_file_count: 0,
            },
            ci: CiInfo {
                provider: None,
                config_files: vec![],
            },
            git: GitInfo {
                is_git_repo: true,
                default_branch: Some("main".into()),
                remote_url: None,
                commit_count: 10,
                recent_contributors: vec![],
            },
            dependencies: DependencySummary::default(),
            directory_tree: String::new(),
            has_maestro_config: false,
            has_workflow_docs: false,
        }
    }

    fn make_mock_report() -> AdaptReport {
        AdaptReport {
            summary: "Test project".into(),
            modules: vec![],
            tech_debt_items: vec![],
        }
    }

    fn make_mock_plan() -> AdaptPlan {
        AdaptPlan {
            milestones: vec![],
            maestro_toml_patch: None,
            workflow_guide: None,
        }
    }

    fn make_mock_materialize_result() -> MaterializeResult {
        MaterializeResult {
            milestones_created: vec![],
            issues_created: vec![],
            tech_debt_issue: None,
            dry_run: false,
        }
    }

    // -- new() defaults --

    #[test]
    fn new_starts_at_configure() {
        let screen = AdaptScreen::new();
        assert_eq!(screen.step, AdaptStep::Configure);
        assert_eq!(screen.selected_field, 0);
        assert!(!screen.cancelled);
    }

    // -- Configure input --

    #[test]
    fn configure_j_moves_field_down() {
        let mut screen = AdaptScreen::new();
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected_field, 1);
    }

    #[test]
    fn configure_k_moves_field_up() {
        let mut screen = AdaptScreen::new();
        screen.selected_field = 2;
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected_field, 1);
    }

    #[test]
    fn configure_k_at_zero_stays_at_zero() {
        let mut screen = AdaptScreen::new();
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected_field, 0);
    }

    #[test]
    fn configure_j_does_not_exceed_max() {
        let mut screen = AdaptScreen::new();
        screen.selected_field = FIELD_COUNT - 1;
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected_field, FIELD_COUNT - 1);
    }

    #[test]
    fn configure_space_toggles_dry_run() {
        let mut screen = AdaptScreen::new();
        screen.selected_field = 1;
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        assert!(screen.config.dry_run);
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        assert!(!screen.config.dry_run);
    }

    #[test]
    fn configure_space_toggles_scan_only() {
        let mut screen = AdaptScreen::new();
        screen.selected_field = 2;
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        assert!(screen.config.scan_only);
    }

    #[test]
    fn configure_space_toggles_no_issues() {
        let mut screen = AdaptScreen::new();
        screen.selected_field = 3;
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        assert!(screen.config.no_issues);
    }

    #[test]
    fn configure_enter_starts_pipeline() {
        let mut screen = AdaptScreen::new();
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert!(matches!(action, ScreenAction::StartAdaptPipeline(_)));
    }

    #[test]
    fn configure_esc_pops() {
        let mut screen = AdaptScreen::new();
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    // -- Progress input --

    #[test]
    fn progress_esc_cancels_and_pops() {
        let mut screen = AdaptScreen::new();
        screen.step = AdaptStep::Scanning;
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
        assert!(screen.cancelled);
    }

    // -- Complete input --

    #[test]
    fn complete_j_scrolls_down() {
        let mut screen = AdaptScreen::new();
        screen.step = AdaptStep::Complete;
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.scroll_offset, 1);
    }

    #[test]
    fn complete_k_scrolls_up() {
        let mut screen = AdaptScreen::new();
        screen.step = AdaptStep::Complete;
        screen.scroll_offset = 3;
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.scroll_offset, 2);
    }

    #[test]
    fn complete_esc_pops() {
        let mut screen = AdaptScreen::new();
        screen.step = AdaptStep::Complete;
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    // -- Failed input --

    #[test]
    fn failed_r_retries() {
        let mut screen = AdaptScreen::new();
        screen.set_error(AdaptStep::Analyzing, "timeout".into());
        assert_eq!(screen.step, AdaptStep::Failed);

        let action = screen.handle_input(&key_event(KeyCode::Char('r')), InputMode::Normal);
        assert!(matches!(action, ScreenAction::StartAdaptPipeline(_)));
        assert_eq!(screen.step, AdaptStep::Analyzing);
        assert!(screen.error.is_none());
    }

    #[test]
    fn failed_esc_pops() {
        let mut screen = AdaptScreen::new();
        screen.step = AdaptStep::Failed;
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    // -- Setters --

    #[test]
    fn set_scan_result_stores_profile() {
        let mut screen = AdaptScreen::new();
        screen.set_scan_result(make_mock_profile());
        assert!(screen.results.profile.is_some());
    }

    #[test]
    fn set_analyze_result_stores_report() {
        let mut screen = AdaptScreen::new();
        screen.set_analyze_result(make_mock_report());
        assert!(screen.results.report.is_some());
    }

    #[test]
    fn set_plan_result_stores_plan() {
        let mut screen = AdaptScreen::new();
        screen.set_plan_result(make_mock_plan());
        assert!(screen.results.plan.is_some());
    }

    #[test]
    fn set_materialize_result_stores_result() {
        let mut screen = AdaptScreen::new();
        screen.set_materialize_result(make_mock_materialize_result());
        assert!(screen.results.materialize.is_some());
    }

    #[test]
    fn set_error_transitions_to_failed() {
        let mut screen = AdaptScreen::new();
        screen.set_error(AdaptStep::Planning, "bad".into());
        assert_eq!(screen.step, AdaptStep::Failed);
        assert_eq!(screen.error.as_ref().unwrap().phase, AdaptStep::Planning);
        assert_eq!(screen.error.as_ref().unwrap().message, "bad");
    }

    // -- tick --

    #[test]
    fn tick_increments_spinner() {
        let mut screen = AdaptScreen::new();
        assert_eq!(screen.spinner_tick, 0);
        screen.tick();
        assert_eq!(screen.spinner_tick, 1);
    }

    // -- build_adapt_config --

    #[test]
    fn build_adapt_config_reflects_form_state() {
        let mut screen = AdaptScreen::new();
        screen.config.dry_run = true;
        screen.config.path = "/tmp/test".into();
        let config = screen.build_adapt_config();
        assert!(config.dry_run);
        assert_eq!(config.path, std::path::PathBuf::from("/tmp/test"));
    }

    // ── resume_step tests ──────────────────────────────────────────────

    #[test]
    fn resume_step_scanning_when_no_cache() {
        let results = AdaptResults::default();
        assert_eq!(results.resume_step(), AdaptStep::Scanning);
    }

    #[test]
    fn resume_step_analyzing_when_profile_cached() {
        let results = AdaptResults {
            profile: Some(make_mock_profile()),
            ..Default::default()
        };
        assert_eq!(results.resume_step(), AdaptStep::Analyzing);
    }

    // ── complete_analyze → Consolidating ────────────────────────────

    #[test]
    fn complete_analyze_transitions_to_consolidating() {
        let mut screen = AdaptScreen::new();
        screen.results.profile = Some(make_mock_profile());
        let cmd = screen.complete_analyze(make_mock_report());
        assert_eq!(screen.step, AdaptStep::Consolidating);
        assert!(matches!(
            cmd,
            Some(crate::tui::app::TuiCommand::RunAdaptConsolidate(_, _, _))
        ));
    }

    #[test]
    fn complete_analyze_with_no_issues_skips_consolidating() {
        let mut screen = AdaptScreen::new();
        screen.config.no_issues = true;
        screen.results.profile = Some(make_mock_profile());
        let cmd = screen.complete_analyze(make_mock_report());
        assert_eq!(screen.step, AdaptStep::Complete);
        assert!(cmd.is_none());
    }

    // ── complete_consolidate ─────────────────────────────────────────

    #[test]
    fn complete_consolidate_transitions_to_planning() {
        let mut screen = AdaptScreen::new();
        screen.step = AdaptStep::Consolidating;
        screen.results.profile = Some(make_mock_profile());
        screen.results.report = Some(make_mock_report());
        let cmd = screen.complete_consolidate("# PRD".into());
        assert_eq!(screen.step, AdaptStep::Planning);
        assert!(matches!(
            cmd,
            Some(crate::tui::app::TuiCommand::RunAdaptPlan(_, _, _, _))
        ));
    }

    #[test]
    fn complete_consolidate_stores_prd_content() {
        let mut screen = AdaptScreen::new();
        screen.step = AdaptStep::Consolidating;
        screen.results.profile = Some(make_mock_profile());
        screen.results.report = Some(make_mock_report());
        screen.complete_consolidate("# Generated PRD".into());
        assert_eq!(
            screen.results.prd_content.as_deref(),
            Some("# Generated PRD")
        );
    }

    #[test]
    fn complete_consolidate_without_profile_returns_none() {
        let mut screen = AdaptScreen::new();
        screen.step = AdaptStep::Consolidating;
        screen.results.profile = None;
        screen.results.report = Some(make_mock_report());
        let cmd = screen.complete_consolidate("# PRD".into());
        assert!(cmd.is_none());
    }

    // ── resume_step with Consolidating ────────────────────────────────

    #[test]
    fn resume_step_consolidating_when_report_cached() {
        let results = AdaptResults {
            profile: Some(make_mock_profile()),
            report: Some(make_mock_report()),
            ..Default::default()
        };
        assert_eq!(results.resume_step(), AdaptStep::Consolidating);
    }

    #[test]
    fn resume_step_planning_when_prd_cached() {
        let results = AdaptResults {
            profile: Some(make_mock_profile()),
            report: Some(make_mock_report()),
            prd_content: Some("# PRD".into()),
            ..Default::default()
        };
        assert_eq!(results.resume_step(), AdaptStep::Planning);
    }

    #[test]
    fn resume_step_materializing_when_plan_cached() {
        let results = AdaptResults {
            profile: Some(make_mock_profile()),
            report: Some(make_mock_report()),
            plan: Some(make_mock_plan()),
            ..Default::default()
        };
        assert_eq!(results.resume_step(), AdaptStep::Materializing);
    }
}
