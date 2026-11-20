//! Fit one styled row to a width (#1399): text that does not fit ends with
//! `…` instead of being cut mid-word at the panel edge. The span-level
//! sibling of `screens::landing::draw::ellipsize` (plain `&str`).

use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// `line` unchanged when it fits `width` columns; otherwise cut so the last
/// visible column is `…`, keeping every span's style.
pub fn ellipsize_line(line: Line<'_>, width: usize) -> Line<'_> {
    if line.width() <= width {
        return line;
    }
    let mut out = Line {
        style: line.style,
        alignment: line.alignment,
        ..Line::default()
    };
    if width == 0 {
        return out;
    }
    // Columns left before the `…`.
    let mut room = width - 1;
    let mut last_style = line.style;
    for span in line.spans {
        last_style = span.style;
        let w = span.content.width();
        if w <= room {
            room -= w;
            out.spans.push(span);
            continue;
        }
        let mut cut = String::new();
        for ch in span.content.chars() {
            let cw = ch.width().unwrap_or(0);
            if cw > room {
                break;
            }
            room -= cw;
            cut.push(ch);
        }
        if !cut.is_empty() {
            out.spans.push(Span::styled(cut, span.style));
        }
        break;
    }
    out.spans.push(Span::styled("…", last_style));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Style};

    fn text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn a_line_that_fits_is_unchanged() {
        let line = Line::from(vec![Span::raw("ab"), Span::raw("cd")]);
        assert_eq!(text(&ellipsize_line(line, 4)), "abcd");
    }

    #[test]
    fn a_long_line_ends_with_an_ellipsis_at_the_width() {
        let red = Style::default().fg(Color::Red);
        let line = Line::from(vec![
            Span::raw("tokens "),
            Span::styled("0 in / 0 out", red),
        ]);
        let fitted = ellipsize_line(line, 12);
        assert_eq!(text(&fitted), "tokens 0 in…");
        assert_eq!(fitted.width(), 12);
        assert_eq!(fitted.spans[1].style, red, "the cut span keeps its style");
    }

    #[test]
    fn a_cut_on_a_span_boundary_still_shows_the_ellipsis() {
        let line = Line::from(vec![Span::raw("abc"), Span::raw("def")]);
        assert_eq!(text(&ellipsize_line(line, 4)), "abc…");
    }

    #[test]
    fn wide_chars_are_not_split() {
        let line = Line::from("日本語");
        let fitted = ellipsize_line(line, 4);
        assert_eq!(text(&fitted), "日…");
        assert!(fitted.width() <= 4);
    }

    #[test]
    fn zero_and_one_columns() {
        assert_eq!(text(&ellipsize_line(Line::from("abc"), 0)), "");
        assert_eq!(text(&ellipsize_line(Line::from("abc"), 1)), "…");
    }
}
