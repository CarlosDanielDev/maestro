use super::{FieldId, IssueType, IssueWizardScreen, IssueWizardStep};
use crate::tui::theme::Theme;
use crate::tui::widgets::{BrailleSpinner, WizardFrame, WizardFrameFooter, WizardFrameHeader};
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
        WizardFrame::draw(
            f,
            area,
            theme,
            WizardFrameHeader {
                step_index: step.index(),
                step_total: IssueWizardStep::total(),
                step_label: step.label(),
            },
            WizardFrameFooter {
                validation_error: self.validation_error(),
                hints: issue_footer_hints(step),
            },
            |f, body_area| match step {
                IssueWizardStep::Context => self.draw_context(f, body_area, theme),
                IssueWizardStep::TypeSelect => self.draw_type_select(f, body_area, theme),
                IssueWizardStep::BasicInfo => self.draw_basic_info(f, body_area),
                IssueWizardStep::DorFields => self.draw_dor_fields(f, body_area),
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

    fn draw_context(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Issue Creation Contract", false);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let lines = vec![
            Line::from(vec![
                Span::styled(
                    "Maestro issues are execution contracts. ",
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "Write the smallest useful unit an agent can ship, test, and review.",
                    Style::default().fg(theme.text_secondary),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("1. ", Style::default().fg(theme.accent_identifier)),
                Span::styled("Classify", Style::default().fg(theme.text_primary)),
                Span::styled(
                    " the work as feature or bug so the right DOR fields appear.",
                    Style::default().fg(theme.text_secondary),
                ),
            ]),
            Line::from(vec![
                Span::styled("2. ", Style::default().fg(theme.accent_identifier)),
                Span::styled("Describe", Style::default().fg(theme.text_primary)),
                Span::styled(
                    " expected behavior, acceptance criteria, files, and test hints.",
                    Style::default().fg(theme.text_secondary),
                ),
            ]),
            Line::from(vec![
                Span::styled("3. ", Style::default().fg(theme.accent_identifier)),
                Span::styled("Connect blockers", Style::default().fg(theme.text_primary)),
                Span::styled(
                    " so the queue respects sequencing instead of guessing.",
                    Style::default().fg(theme.text_secondary),
                ),
            ]),
            Line::from(vec![
                Span::styled("4. ", Style::default().fg(theme.accent_identifier)),
                Span::styled("Review", Style::default().fg(theme.text_primary)),
                Span::styled(
                    " the final markdown before it is sent to GitHub.",
                    Style::default().fg(theme.text_secondary),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Press Enter when the issue should become clear enough for another session to execute.",
                Style::default()
                    .fg(theme.accent_warning)
                    .add_modifier(Modifier::BOLD),
            )),
        ];
        f.render_widget(
            Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }),
            inner,
        );
    }

    fn draw_type_select(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(centered_rect(area, 60, 7));

        let pick = |label: &'static str, selected: bool| -> Paragraph<'static> {
            let style = selection_style(theme, selected, theme.text_primary);
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(label, style)),
                Line::from(""),
            ])
            .alignment(Alignment::Center)
            .block(theme.styled_block(label, selected))
        };

        f.render_widget(
            pick("Feature", self.payload().issue_type == IssueType::Feature),
            chunks[0],
        );
        f.render_widget(
            pick("Bug", self.payload().issue_type == IssueType::Bug),
            chunks[1],
        );
    }

    fn draw_basic_info(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(3)])
            .split(area);
        self.draw_field(f, chunks[0], FieldId::Title);
        self.draw_field(f, chunks[1], FieldId::Overview);
    }

    fn draw_dor_fields(&self, f: &mut Frame, area: Rect) {
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
            self.draw_field(f, chunks[i], *field);
        }
    }

    fn draw_preview(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Preview  (Enter creates on GitHub, Esc revises)", false);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(Span::styled(
            self.payload().title.clone(),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        )));
        let labels = self.render_labels();
        lines.push(Line::from(vec![
            Span::styled("Labels: ", Style::default().fg(theme.text_secondary)),
            Span::styled(labels.join(", "), Style::default().fg(theme.accent_info)),
        ]));
        if let Some(m) = self.payload().milestone {
            lines.push(Line::from(vec![
                Span::styled("Milestone: ", Style::default().fg(theme.text_secondary)),
                Span::styled(
                    format!("#{}", m),
                    Style::default().fg(theme.accent_identifier),
                ),
            ]));
        }
        lines.push(Line::from(""));
        for raw in self
            .render_body_markdown()
            .lines()
            .take(inner.height.saturating_sub(4) as usize)
        {
            lines.push(Line::from(raw.to_string()));
        }
        f.render_widget(Paragraph::new(lines), inner);
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
                "r: retry  Esc: back to Preview",
                Style::default().fg(theme.text_secondary),
            )),
        ];
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    }

    fn draw_dependencies(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block(
            "Blocked By  (Space toggles, j/k navigates, Enter advances)",
            false,
        );
        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.dep_loading() {
            f.render_widget(
                Paragraph::new(BrailleSpinner::render(
                    self.spinner_tick(),
                    "Loading open issues…",
                    self.use_nerd_font(),
                    theme,
                )),
                inner,
            );
            return;
        }

        let Some(issues) = self.dep_issues() else {
            f.render_widget(
                Paragraph::new(Span::styled(
                    "  Press Enter to continue without dependencies.",
                    Style::default().fg(theme.text_secondary),
                )),
                inner,
            );
            return;
        };

        if issues.is_empty() {
            f.render_widget(
                Paragraph::new(Span::styled(
                    "  No open issues found.",
                    Style::default().fg(theme.text_secondary),
                )),
                inner,
            );
            return;
        }

        let lines: Vec<Line> = issues
            .iter()
            .enumerate()
            .take(inner.height as usize)
            .map(|(i, issue)| {
                let selected = i == self.dep_selected();
                let style = selection_style(theme, selected, theme.text_primary);
                let check = if self.dep_is_checked(issue.number) {
                    "[x]"
                } else {
                    "[ ]"
                };
                let labels = if issue.labels.is_empty() {
                    String::new()
                } else {
                    let names: Vec<&str> =
                        issue.labels.iter().map(|s| s.as_str()).take(3).collect();
                    format!("  ({})", names.join(", "))
                };
                Line::from(vec![
                    Span::styled("  ", style),
                    Span::styled(
                        check.to_string(),
                        if selected {
                            style
                        } else {
                            style.fg(theme.accent_success)
                        },
                    ),
                    Span::styled(
                        format!(" #{} ", issue.number),
                        if selected {
                            style
                        } else {
                            style.fg(theme.accent_identifier)
                        },
                    ),
                    Span::styled(issue.title.clone(), style),
                    Span::styled(
                        labels,
                        if selected {
                            style
                        } else {
                            style.fg(theme.text_secondary)
                        },
                    ),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_field(&self, f: &mut Frame, area: Rect, field: FieldId) {
        // The textarea owns its own block (set by `refresh_field_blocks`
        // on the mutable draw entry point) and its own cursor rendering.
        let step_fields = self.step_fields();
        let Some(idx) = step_fields.iter().position(|f| *f == field) else {
            return;
        };
        let Some(ta_field) = self.fields.get(idx) else {
            return;
        };
        f.render_widget(ta_field.area(), area);
    }
}

fn selection_style(theme: &Theme, selected: bool, default_fg: Color) -> Style {
    if selected {
        Style::default()
            .fg(theme.selection_fg)
            .bg(theme.selection_bg)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default().fg(default_fg)
    }
}

fn issue_footer_hints(step: IssueWizardStep) -> Option<&'static str> {
    match step {
        IssueWizardStep::Context => Some("[Enter] start issue contract   [Esc] back"),
        IssueWizardStep::TypeSelect => {
            Some("[←/→ h/l] choose Feature or Bug   [Enter] next   [Esc] back")
        }
        IssueWizardStep::Dependencies => {
            Some("[j/k ↑/↓] move   [Space] toggle blocker   [Enter] next   [Esc] back")
        }
        IssueWizardStep::AiReview => {
            Some("[r] revise   [s] skip   [i] improve   [Enter] next   [Esc] back")
        }
        IssueWizardStep::Preview => Some("[Enter] create on GitHub   [Esc] revise"),
        IssueWizardStep::Creating => Some("Creating issue on GitHub…"),
        IssueWizardStep::Complete => Some("[Enter] create another   [Esc] return to Landing"),
        IssueWizardStep::Failed => Some("[r] retry   [Esc] back to Preview"),
        IssueWizardStep::BasicInfo => Some(
            "[Tab/BackTab] switch Title/Overview   [Shift+Enter] newline in Overview   [Enter] next   [Esc] back",
        ),
        IssueWizardStep::DorFields => Some(
            "[Tab/BackTab] switch DOR fields   [Shift+Enter] newline   [Enter] next   [Esc] back",
        ),
    }
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
