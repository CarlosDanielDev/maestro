use crate::tui::ds::bar_row::draw_left_right;
use crate::tui::ds::breakpoint::Breakpoint;
use crate::tui::ds::checklist::{CheckItem, checklist_panel, ready_strip};
use crate::tui::ds::stepper::{self, Steps};
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

pub const FOOTER_HINTS: &str = "[Enter] next   [Tab] cycle   [Shift+Enter] newline   [Esc] back";

#[derive(Debug, Clone, Copy)]
pub struct WizardFrameHeader<'a> {
    pub step_index: usize,
    pub step_total: usize,
    pub step_label: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct WizardFrameFooter<'a> {
    pub validation_error: Option<&'a str>,
    pub hints: Option<&'a str>,
}

/// Header of the shared wizard frame (`WizardFrame::draw_stepped`, #1274):
/// the one-row stepper and the right-aligned state slot (`● draft saved 12s
/// ago`, a writes ledger). Each wizard owns its draft; the frame only shows
/// the string.
#[derive(Debug, Clone)]
pub struct WizardStepHeader<'a> {
    pub steps: &'a [&'a str],
    pub current: usize,
    pub errors: &'a [usize],
    pub state: Option<Span<'a>>,
}

/// The live readiness checklist beside the body: a `ds::checklist_panel` on
/// Wide, a `ds::ready_strip` under the stepper on Regular and Compact.
#[derive(Debug, Clone, Copy)]
pub struct WizardChecklist<'a> {
    pub title: &'a str,
    pub items: &'a [CheckItem<'a>],
}

/// Width of the checklist column on Wide (`Fill(1), Length(42)`).
pub const CHECKLIST_WIDTH: u16 = 42;

/// Rects of the shared wizard frame. `strip` and `checklist` are `None` when
/// there is no checklist; at most one of them is `Some`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WizardLayout {
    pub stepper: Rect,
    pub strip: Option<Rect>,
    pub body: Rect,
    pub checklist: Option<Rect>,
}

/// Split `area`: stepper row, then the body. With a checklist, Wide adds the
/// 42-column column on the right; other widths add the one-row strip.
pub fn layout(area: Rect, bp: Breakpoint, has_checklist: bool) -> WizardLayout {
    let wide = bp == Breakpoint::Wide;
    let strip_h = u16::from(has_checklist && !wide);
    let [stepper, strip, rest] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(strip_h),
        Constraint::Fill(1),
    ])
    .areas(area);
    let (body, checklist) = if has_checklist && wide {
        let [body, column] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(CHECKLIST_WIDTH)])
                .areas(rest);
        (body, Some(column))
    } else {
        (rest, None)
    };
    WizardLayout {
        stepper,
        strip: (strip_h > 0).then_some(strip),
        body,
        checklist,
    }
}

/// A form field's border: Thick when focused, Plain otherwise (`ds::panel`).
/// An error turns the border `accent_error` and sits on the bottom border as
/// `✗ message`, so it reads under NO_COLOR too. Replaces the frame's old
/// footer error line.
pub fn field_block<'a>(
    label: &'a str,
    focused: bool,
    error: Option<&'a str>,
    theme: &Theme,
) -> Block<'a> {
    let block = theme.styled_block(label, focused);
    match error {
        Some(err) => {
            let red = Style::default().fg(theme.accent_error);
            block
                .border_style(red)
                .title_bottom(Line::from(Span::styled(
                    format!(" ✗ {err} "),
                    red.add_modifier(Modifier::BOLD),
                )))
        }
        None => block,
    }
}

pub struct WizardFrame;

impl WizardFrame {
    pub fn draw<F>(
        f: &mut Frame,
        area: Rect,
        theme: &Theme,
        header: WizardFrameHeader<'_>,
        footer: WizardFrameFooter<'_>,
        body_callback: F,
    ) where
        F: FnOnce(&mut Frame, Rect),
    {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(area);

        Self::draw_header(f, chunks[0], theme, header);
        body_callback(f, chunks[1]);
        Self::draw_footer(f, chunks[2], theme, footer);
    }

    /// The shared wizard frame (design §02.2/§02.3): stepper row with the
    /// state slot, body, and the checklist (column on Wide, strip otherwise).
    /// No footer: hints come from `Screen::footer_hints`, and errors sit on
    /// the field borders (`field_block`). The shell CrumbBar stays in
    /// `ui::draw`.
    pub fn draw_stepped<F>(
        f: &mut Frame,
        area: Rect,
        theme: &Theme,
        header: WizardStepHeader<'_>,
        checklist: Option<WizardChecklist<'_>>,
        body_callback: F,
    ) where
        F: FnOnce(&mut Frame, Rect),
    {
        let bp = Breakpoint::from_area(f.area());
        let rects = layout(area, bp, checklist.is_some());
        let steps = Steps {
            labels: header.steps,
            current: header.current,
            errors: header.errors,
        };
        let row = rects.stepper.width;
        let right = header
            .state
            .map(|span| Line::from(vec![span, Span::raw(" ")]))
            .unwrap_or_default();
        // The stepper gets the room left of the state slot (one column gap),
        // so it collapses or ends in `…` before the state is dropped.
        let right_w = right.width() as u16;
        let room = if right_w > 0 && right_w + 1 < row {
            row - right_w - 1
        } else {
            row
        };
        let left = stepper::stepper_line(steps, row, room, theme);
        draw_left_right(f, rects.stepper, left, right);
        if let Some(list) = checklist {
            if let Some(strip) = rects.strip {
                let compact = bp != Breakpoint::Regular;
                let line = ready_strip(list.title, list.items, compact, strip.width, theme);
                f.render_widget(Paragraph::new(line), strip);
            }
            if let Some(column) = rects.checklist {
                checklist_panel(f, column, list.title, list.items, false, theme);
            }
        }
        body_callback(f, rects.body);
    }

    fn draw_header(f: &mut Frame, area: Rect, theme: &Theme, header: WizardFrameHeader<'_>) {
        let title = format!(
            "Step {}/{}: {}",
            header.step_index, header.step_total, header.step_label
        );
        let block = theme.styled_block(&title, false);
        f.render_widget(Paragraph::new("").block(block), area);
    }

    fn draw_footer(f: &mut Frame, area: Rect, theme: &Theme, footer: WizardFrameFooter<'_>) {
        let hint = footer.hints.unwrap_or(FOOTER_HINTS);
        let lines = if let Some(err) = footer.validation_error {
            vec![
                Line::from(Span::styled(
                    err,
                    Style::default()
                        .fg(theme.accent_error)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    hint,
                    Style::default()
                        .fg(theme.text_secondary)
                        .add_modifier(Modifier::DIM),
                )),
            ]
        } else {
            vec![Line::from(Span::styled(
                hint,
                Style::default()
                    .fg(theme.text_secondary)
                    .add_modifier(Modifier::DIM),
            ))]
        };
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
    }
}

#[cfg(test)]
#[path = "wizard_frame_tests.rs"]
mod tests;
