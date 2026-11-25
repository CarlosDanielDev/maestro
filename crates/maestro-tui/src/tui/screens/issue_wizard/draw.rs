use super::{FieldId, IssueType, IssueWizardScreen, IssueWizardStep};
use crate::tui::ds::checklist::CheckItem;
use crate::tui::theme::Theme;
use crate::tui::widgets::wizard_frame::field_block;
use crate::tui::widgets::{BrailleSpinner, WizardChecklist, WizardFrame, WizardStepHeader};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

impl IssueWizardScreen {
    pub(super) fn draw_impl(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let step = self.step();
        let labels = IssueWizardStep::STEPPER.map(|s| s.label());
        let errors = self.stepper_errors();
        let kind = match self.payload().issue_type {
            IssueType::Feature => "feature",
            IssueType::Bug => "bug",
        };
        let rows = self.checklist_rows();
        let items: Vec<CheckItem> = rows
            .iter()
            .map(|(label, state, note)| CheckItem {
                label,
                state: *state,
                note: note.as_deref(),
            })
            .collect();
        WizardFrame::draw_stepped(
            f,
            area,
            theme,
            WizardStepHeader {
                steps: &labels,
                current: step.stepper_index(),
                errors: &errors,
                state: Some(Span::styled(kind, Style::default().fg(theme.text_muted))),
            },
            Some(WizardChecklist {
                title: "Draft",
                items: &items,
            }),
            |f, body_area| match step {
                IssueWizardStep::BasicInfo => self.draw_basic_info(f, body_area, theme),
                IssueWizardStep::DorFields => self.draw_dor_fields(f, body_area, theme),
                IssueWizardStep::Dependencies => self.draw_dependencies(f, body_area, theme),
                IssueWizardStep::AiReview => self.draw_ai_review(f, body_area, theme),
                IssueWizardStep::Preview => self.draw_preview(f, body_area, theme),
                IssueWizardStep::Creating => self.draw_creating(f, body_area, theme),
                IssueWizardStep::Complete => self.draw_complete(f, body_area, theme),
                IssueWizardStep::Failed => self.draw_failed(f, body_area, theme),
            },
        );

        // #455 — already-exists modal overlays everything else.
        if self.already_exists_modal().is_some() {
            self.draw_already_exists_modal(f, area, theme);
        }
    }

    /// #455 — blocking modal rendered on top of the wizard when a
    /// duplicate-title pre-check matched an existing issue.
    fn draw_already_exists_modal(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let Some(modal) = self.already_exists_modal() else {
            return;
        };
        // Center a small modal rect inside `area`.
        let w = area.width.min(66);
        let h = 9u16;
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;
        let rect = Rect {
            x,
            y,
            width: w,
            height: h,
        };
        f.render_widget(ratatui::widgets::Clear, rect);

        let block = theme.styled_block("Issue already exists", true);
        let inner = block.inner(rect);
        f.render_widget(block, rect);

        let header = Line::from(vec![
            Span::raw("An issue with this title already exists: "),
            Span::styled(
                format!("#{} ({})", modal.number, modal.state),
                Style::default()
                    .fg(theme.accent_warning)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        let title_line = Line::from(vec![
            Span::raw("Title: "),
            Span::styled(&modal.title, Style::default().add_modifier(Modifier::BOLD)),
        ]);

        let (edit_style, cancel_style) = if modal.focus == 0 {
            (
                selection_style(theme, true, theme.text_primary),
                Style::default()
                    .fg(theme.text_secondary)
                    .add_modifier(Modifier::DIM),
            )
        } else {
            (
                Style::default()
                    .fg(theme.text_secondary)
                    .add_modifier(Modifier::DIM),
                selection_style(theme, true, theme.text_primary),
            )
        };

        let buttons = Line::from(vec![
            Span::styled(" [Edit title] ", edit_style),
            Span::raw("   "),
            Span::styled(" [Cancel] ", cancel_style),
        ]);

        let lines = vec![
            header,
            Line::from(""),
            title_line,
            Line::from(""),
            buttons,
            Line::from(""),
            Line::from(Span::styled(
                "← → to switch, Enter to confirm, Esc to cancel",
                Style::default()
                    .fg(theme.text_secondary)
                    .add_modifier(Modifier::DIM),
            )),
        ];

        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    }

    /// Basics: Title, the Type row, Overview (Tab order too).
    fn draw_basic_info(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let [title, kind, overview] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(3),
        ])
        .areas(area);
        self.draw_field(f, title, FieldId::Title, theme);
        self.draw_type_row(f, kind, theme);
        self.draw_field(f, overview, FieldId::Overview, theme);
        if self.leave_armed() {
            // On the Overview bottom border: what the second Esc does.
            let row = Rect {
                y: overview.bottom().saturating_sub(1),
                height: 1,
                ..overview
            };
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    " Esc again closes and drops this draft · any other key keeps it ",
                    Style::default()
                        .fg(theme.accent_warning)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED),
                )))
                .alignment(Alignment::Center),
                row,
            );
        }
    }

    /// The Type control: `● Feature  ○ Bug` and what Bug adds.
    fn draw_type_row(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let focused = self.type_focused;
        let bug = self.payload().issue_type == IssueType::Bug;
        let pick = |label: &'static str, on: bool| {
            let glyph = if on { "● " } else { "○ " };
            let style = if on {
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_secondary)
            };
            Span::styled(format!("{glyph}{label}"), style)
        };
        let line = Line::from(vec![
            Span::raw(" "),
            pick("Feature", !bug),
            Span::raw("   "),
            pick("Bug", bug),
            Span::styled(
                "   Bug adds Current behavior, Steps to reproduce and the bug label",
                Style::default().fg(theme.text_muted),
            ),
        ]);
        let block = theme.styled_block("Type  (←/→ f/b)", focused);
        f.render_widget(Paragraph::new(line).block(block), area);
    }

    fn draw_dor_fields(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let fields = self.step_fields();
        if fields.is_empty() {
            return;
        }
        let constraints: Vec<Constraint> = fields
            .iter()
            .map(|f| {
                if f.is_multiline() {
                    Constraint::Min(3)
                } else {
                    Constraint::Length(3)
                }
            })
            .collect();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);
        for (i, field) in fields.iter().enumerate() {
            self.draw_field(f, chunks[i], *field, theme);
        }
    }

    fn draw_creating(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Creating", false);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let lines = vec![
            Line::from(""),
            BrailleSpinner::render(
                self.spinner_tick(),
                "Creating issue on GitHub…",
                self.use_nerd_font(),
                theme,
            ),
        ];
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    }

    fn draw_complete(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Complete", false);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let mut lines: Vec<Line> = Vec::new();
        if let Some(num) = self.created_issue_number() {
            lines.push(Line::from(Span::styled(
                format!("Issue #{} created successfully", num),
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Enter: create another  Esc: return to Landing",
            Style::default().fg(theme.text_secondary),
        )));
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    }

    fn draw_failed(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Failed", false);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                self.create_error().unwrap_or("Unknown error").to_string(),
                Style::default().fg(theme.accent_error),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Esc: back to the create gate (⏎ ⏎ retries)",
                Style::default().fg(theme.text_secondary),
            )),
        ];
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    }

    /// A form field: `field_block` (Thick on focus, `*` on required labels,
    /// the error on the bottom border once touched) around its textarea.
    fn draw_field(&self, f: &mut Frame, area: Rect, field: FieldId, theme: &Theme) {
        let step_fields = self.step_fields();
        let Some(idx) = step_fields.iter().position(|f| *f == field) else {
            return;
        };
        let Some(ta_field) = self.fields.get(idx) else {
            return;
        };
        let focused = !self.type_focused && self.fields.focus() == idx;
        let error = self.field_error(field);
        let block = field_block(form_label(field), focused, error.as_deref(), theme);
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(ta_field.area(), inner);
    }
}

/// Field label with `*` on the required ones.
fn form_label(field: FieldId) -> &'static str {
    match field {
        FieldId::Title => "Title *",
        FieldId::Overview => "Overview *",
        FieldId::ExpectedBehavior => "Expected Behavior *",
        FieldId::AcceptanceCriteria => "Acceptance Criteria *",
        other => other.label(),
    }
}

pub(super) fn selection_style(theme: &Theme, selected: bool, default_fg: Color) -> Style {
    if selected {
        Style::default()
            .fg(theme.selection_fg)
            .bg(theme.selection_bg)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default().fg(default_fg)
    }
}

/// Shell footer keys for the focused element only (`Screen::footer_hints`).
pub(super) fn footer_hints(s: &IssueWizardScreen) -> Vec<(&'static str, &'static str)> {
    use crate::tui::ds::armed::ArmedState;
    match s.step() {
        IssueWizardStep::BasicInfo if s.leave_armed() => {
            vec![("Esc", "close, drop the draft"), ("any key", "keep it")]
        }
        IssueWizardStep::BasicInfo if s.type_focused => vec![
            ("←→ f b", "type"),
            ("Tab", "Overview"),
            ("Enter", "next"),
            ("Esc", "close"),
        ],
        IssueWizardStep::BasicInfo => vec![
            ("Tab", "field / type"),
            ("Shift+Enter", "newline"),
            ("Enter", "next"),
            ("Esc", "close"),
        ],
        IssueWizardStep::DorFields => vec![
            ("Tab", "field"),
            ("Shift+Enter", "newline"),
            ("Enter", "next"),
            ("Esc", "back"),
        ],
        IssueWizardStep::Dependencies if s.dep_filter_editing() => vec![
            ("↑↓", "move"),
            ("Enter", "keep filter"),
            ("Esc", "clear filter"),
        ],
        IssueWizardStep::Dependencies => vec![
            ("/", "filter"),
            ("j k", "move"),
            ("Space", "toggle"),
            ("Enter", "next"),
            ("Esc", "back"),
        ],
        IssueWizardStep::AiReview if s.improve_candidate().is_some() => vec![
            ("j k", "section"),
            ("↑↓", "scroll"),
            ("a", "accept"),
            ("d", "keep mine"),
            ("Enter", "merge accepted"),
            ("r", "re-run (paid)"),
            ("Esc", "discard all"),
        ],
        IssueWizardStep::AiReview if s.review_error().is_some() || s.improve_error().is_some() => {
            vec![
                ("r", "retry (paid AI call)"),
                ("s", "skip"),
                ("e", "revise"),
                ("Esc", "back"),
            ]
        }
        IssueWizardStep::AiReview => vec![
            ("i", "improve (paid AI call)"),
            ("e", "revise"),
            ("s", "skip"),
            ("Enter", "next"),
            ("Esc", "back"),
        ],
        IssueWizardStep::Preview => match s.create_gate().state() {
            ArmedState::Blocked(_) => vec![("Enter", "go to the problem"), ("Esc", "back")],
            ArmedState::Armed(_) => vec![("Enter", "create on GitHub"), ("Esc", "disarm")],
            _ => vec![("Enter", "arm create"), ("Esc", "back")],
        },
        IssueWizardStep::Creating => Vec::new(),
        IssueWizardStep::Complete => vec![("Enter", "create another"), ("Esc", "Landing")],
        IssueWizardStep::Failed => vec![("Esc", "back to the gate")],
    }
}
