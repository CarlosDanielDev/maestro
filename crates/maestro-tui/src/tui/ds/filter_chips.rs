//! `ds::filter_chips` (#1256): the always-visible active filters
//! (`filterChips` in `docs/design-system/index.html`, §04 Kit v2), e.g.
//! ` label:tui ✕  is:open ✕ `. Each chip is reversed with a `✕` remove mark;
//! chips that do not fit collapse into one muted `+n`.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui::theme::Theme;

/// One active filter: `key:value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterChip<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

/// The chips as one line no wider than `width`. When chips do not fit, the
/// line ends with `+n`, `n` being the number of chips left out.
pub fn filter_chips(chips: &[FilterChip], width: u16, theme: &Theme) -> Line<'static> {
    let chip_style = Style::new()
        .fg(theme.text_primary)
        .add_modifier(Modifier::REVERSED);
    let width = usize::from(width);
    let mut spans = Vec::new();
    let mut used = 0;
    for (i, chip) in chips.iter().enumerate() {
        let seg = [
            Span::styled(format!(" {}:", chip.key), chip_style),
            Span::styled(
                format!("{} ", chip.value),
                chip_style.add_modifier(Modifier::BOLD),
            ),
            Span::styled("✕ ", chip_style),
            Span::raw(" "),
        ];
        let seg_width: usize = seg.iter().map(Span::width).sum();
        // Keep room for the `+n` of the chips after this one.
        let after = chips.len() - i - 1;
        let reserve = if after > 0 { overflow(after).len() } else { 0 };
        if used + seg_width + reserve > width {
            spans.push(Span::styled(
                overflow(chips.len() - i),
                Style::new().fg(theme.text_muted),
            ));
            break;
        }
        used += seg_width;
        spans.extend(seg);
    }
    Line::from(spans)
}

/// The overflow mark for `n` hidden chips.
fn overflow(n: usize) -> String {
    format!("+{n}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::widgets::Paragraph;

    fn chip<'a>(key: &'a str, value: &'a str) -> FilterChip<'a> {
        FilterChip { key, value }
    }

    /// Seven chips: more than 80 columns hold.
    fn many() -> Vec<FilterChip<'static>> {
        vec![
            chip("label", "tui"),
            chip("is", "open"),
            chip("author", "me"),
            chip("milestone", "v0.33.0"),
            chip("sort", "updated"),
            chip("type", "bug"),
            chip("assignee", "none"),
        ]
    }

    fn text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn filter_chips_all_fit_without_overflow_mark() {
        let line = filter_chips(&many()[..2], 80, &Theme::dark());
        assert_eq!(text(&line), " label:tui ✕   is:open ✕  ");
    }

    #[test]
    fn filter_chips_chip_is_reversed_with_bold_value() {
        let line = filter_chips(&many()[..1], 80, &Theme::no_color());
        let value = line
            .spans
            .iter()
            .find(|s| s.content.contains("tui"))
            .unwrap();
        assert!(value.style.add_modifier.contains(Modifier::REVERSED));
        assert!(value.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn filter_chips_overflow_counts_hidden_chips_and_fits_width() {
        let line = filter_chips(&many(), 80, &Theme::dark());
        let t = text(&line);
        assert!(t.ends_with("+3"), "three chips hidden: {t:?}");
        assert!(line.width() <= 80, "fits 80 columns: {t:?}");
    }

    #[test]
    fn filter_chips_empty_is_empty_line() {
        assert_eq!(filter_chips(&[], 80, &Theme::dark()).width(), 0);
    }

    #[test]
    fn filter_chips_overflow() {
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(80, 1)).unwrap();
        let line = filter_chips(&many(), 80, &theme);
        terminal
            .draw(|f| f.render_widget(Paragraph::new(line), f.area()))
            .unwrap();
        assert_snapshot!(format!("{:?}", terminal.backend().buffer()));
    }
}
