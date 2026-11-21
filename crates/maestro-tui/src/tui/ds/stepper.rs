//! `ds::stepper`: the one-row wizard progress track (`stepper()` in
//! `docs/design-system/index.html`, §02.2 Issue wizard and §02.3 Create
//! milestone). `✓` done · `●` current · `○` ahead · `✗` has errors, with the
//! full labels from 100 columns up and `Step n/N · Label` + a dot track below.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::fit::ellipsize_line;
use crate::tui::theme::Theme;

/// Rows narrower than this draw the collapsed `Step n/N · Label` form.
pub const COLLAPSE_BELOW: u16 = 100;

/// The steps of a flow: labels in order, the current index (0-based) and the
/// indexes that have errors.
#[derive(Debug, Clone, Copy)]
pub struct Steps<'a> {
    pub labels: &'a [&'a str],
    pub current: usize,
    pub errors: &'a [usize],
}

impl Steps<'_> {
    /// Glyph and glyph style of step `i`. An error wins over done and
    /// current; meaning rides on the glyph, so it reads under NO_COLOR.
    fn glyph(&self, i: usize, theme: &Theme) -> (&'static str, Style) {
        let fg = |c| Style::default().fg(c);
        if self.errors.contains(&i) {
            ("✗", fg(theme.accent_error).add_modifier(Modifier::BOLD))
        } else if i < self.current {
            ("✓", fg(theme.accent_success))
        } else if i == self.current {
            ("●", fg(theme.accent_warning).add_modifier(Modifier::BOLD))
        } else {
            ("○", fg(theme.text_muted))
        }
    }

    fn label_style(&self, i: usize, theme: &Theme) -> Style {
        if i == self.current {
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD)
        } else if i < self.current {
            Style::default().fg(theme.text_primary)
        } else {
            Style::default().fg(theme.text_muted)
        }
    }
}

/// The stepper for a row `row_width` columns wide, of which it may use
/// `room` (the rest holds a state slot): `✓ Goals ─ ● Plan ─ ○ …` from
/// [`COLLAPSE_BELOW`] columns up when the labels fit in `room`, else
/// `Step n/N · Label   ✓●○   next Label`, ending in `…` when even that does
/// not fit.
pub fn stepper_line(steps: Steps<'_>, row_width: u16, room: u16, theme: &Theme) -> Line<'static> {
    let muted = Style::default().fg(theme.text_muted);
    let mut full = vec![Span::raw(" ")];
    for (i, label) in steps.labels.iter().enumerate() {
        if i > 0 {
            full.push(Span::styled(" ─ ", muted.add_modifier(Modifier::DIM)));
        }
        let (glyph, style) = steps.glyph(i, theme);
        full.push(Span::styled(format!("{glyph} "), style));
        full.push(Span::styled(label.to_string(), steps.label_style(i, theme)));
    }
    let full = Line::from(full);
    if row_width >= COLLAPSE_BELOW && full.width() < usize::from(room) {
        return full;
    }

    let total = steps.labels.len();
    let current = steps.labels.get(steps.current).copied().unwrap_or_default();
    let mut spans = vec![
        Span::styled(" Step ", muted),
        Span::styled(
            format!("{}/{total}", steps.current + 1),
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · ", muted),
        Span::styled(current.to_string(), steps.label_style(steps.current, theme)),
        Span::raw("   "),
    ];
    for i in 0..total {
        let (glyph, style) = steps.glyph(i, theme);
        spans.push(Span::styled(glyph, style));
    }
    if let Some(next) = steps.labels.get(steps.current + 1) {
        spans.push(Span::styled(format!("   next {next}"), muted));
    }
    ellipsize_line(Line::from(spans), usize::from(room))
}

/// Draw the stepper on the first row of `area`.
pub fn draw(f: &mut Frame, area: Rect, steps: Steps<'_>, theme: &Theme) {
    let line = stepper_line(steps, area.width, area.width, theme);
    f.render_widget(Paragraph::new(line), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use ratatui::{Terminal, backend::TestBackend};

    const LABELS: &[&str] = &["Goals", "References", "Plan", "Create"];

    /// Goals done, References has errors, Plan current, Create ahead.
    fn all_states() -> Steps<'static> {
        Steps {
            labels: LABELS,
            current: 2,
            errors: &[1],
        }
    }

    fn render(width: u16) -> Terminal<TestBackend> {
        let mut t = Terminal::new(TestBackend::new(width, 1)).unwrap();
        let theme = Theme::dark();
        t.draw(|f| draw(f, f.area(), all_states(), &theme)).unwrap();
        t
    }

    fn row(t: &Terminal<TestBackend>) -> String {
        let buf = t.backend().buffer();
        (0..buf.area.width).map(|x| buf[(x, 0)].symbol()).collect()
    }

    #[test]
    fn stepper_wide() {
        let t = render(120);
        let out = row(&t);
        assert!(
            out.starts_with(" ✓ Goals ─ ✗ References ─ ● Plan ─ ○ Create"),
            "{out:?}"
        );
        assert_snapshot!("stepper_wide", t.backend());
    }

    #[test]
    fn stepper_collapsed() {
        let t = render(90);
        let out = row(&t);
        assert!(
            out.starts_with(" Step 3/4 · Plan   ✓✗●○   next Create"),
            "{out:?}"
        );
        assert_snapshot!("stepper_collapsed", t.backend());
    }

    #[test]
    fn stepper_collapses_exactly_below_100_columns() {
        let theme = Theme::dark();
        let text = |w| stepper_line(all_states(), w, w, &theme).to_string();
        assert!(text(COLLAPSE_BELOW).contains("✓ Goals ─"));
        assert!(text(COLLAPSE_BELOW - 1).contains("Step 3/4"));
    }

    #[test]
    fn stepper_collapses_when_labels_do_not_fit() {
        let theme = Theme::dark();
        let long = [
            "A very long first step label",
            "Another very long step label",
            "A third very long step label",
            "Fourth",
        ];
        let steps = Steps {
            labels: &long,
            current: 0,
            errors: &[],
        };
        let out = stepper_line(steps, COLLAPSE_BELOW, COLLAPSE_BELOW, &theme).to_string();
        assert!(
            out.contains("Step 1/4 · A very long first step label"),
            "{out:?}"
        );
    }

    #[test]
    fn error_glyph_wins_over_done_and_current() {
        let theme = Theme::dark();
        let steps = Steps {
            labels: LABELS,
            current: 1,
            errors: &[0, 1],
        };
        let out = stepper_line(steps, 120, 120, &theme).to_string();
        assert!(out.contains("✗ Goals ─ ✗ References"), "{out:?}");
    }

    #[test]
    fn last_step_has_no_next_hint() {
        let theme = Theme::dark();
        let steps = Steps {
            labels: LABELS,
            current: 3,
            errors: &[],
        };
        let out = stepper_line(steps, 90, 90, &theme).to_string();
        assert!(out.contains("Step 4/4 · Create   ✓✓✓●"), "{out:?}");
        assert!(!out.contains("next"), "{out:?}");
    }

    #[test]
    fn collapsed_stepper_ellipsizes_long_labels_at_80_columns() {
        let theme = Theme::dark();
        let long = [
            "Pick the milestone to review today",
            "Check every issue against the definition of ready",
        ];
        let steps = Steps {
            labels: &long,
            current: 0,
            errors: &[],
        };
        let line = stepper_line(steps, 80, 80, &theme);
        let out = line.to_string();
        assert!(line.width() <= 80, "{} cols: {out:?}", line.width());
        assert!(out.ends_with('…'), "{out:?}");
    }

    #[test]
    fn full_form_needs_the_room_not_only_the_row() {
        let theme = Theme::dark();
        let out = stepper_line(all_states(), 120, 40, &theme).to_string();
        assert!(out.starts_with(" Step 3/4 · Plan"), "{out:?}");
        let line = stepper_line(all_states(), 120, 20, &theme);
        assert!(line.width() <= 20, "{:?}", line.to_string());
    }
}
