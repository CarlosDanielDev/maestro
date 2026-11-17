//! `ds::sub_tabs` (#1256): the sub-view strip (`subTabs` in
//! `docs/design-system/index.html`, §04 Kit v2), e.g. the session views
//! `d Detail · f Output · l Log · v Gates · e Events`. The active tab is
//! reversed and bold, so it reads under NO_COLOR too.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui::theme::Theme;

/// One tab: its key letter, its label and an optional badge (`✗`, `12`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubTab<'a> {
    pub key: &'a str,
    pub label: &'a str,
    pub badge: Option<&'a str>,
}

/// The strip as one line. `active` is an index into `tabs`; out of range
/// means no tab is active.
pub fn sub_tabs(tabs: &[SubTab], active: usize, theme: &Theme) -> Line<'static> {
    let on = Style::new()
        .fg(theme.text_primary)
        .add_modifier(Modifier::REVERSED);
    let mut spans = vec![Span::raw(" ")];
    for (i, tab) in tabs.iter().enumerate() {
        let (pad, key, label, badge) = if i == active {
            let bold = on.add_modifier(Modifier::BOLD);
            (on, bold, bold, on)
        } else {
            (
                Style::new(),
                Style::new()
                    .fg(theme.keybind_key)
                    .add_modifier(Modifier::BOLD),
                Style::new().fg(theme.text_primary),
                Style::new().fg(theme.text_muted),
            )
        };
        spans.push(Span::styled(" ", pad));
        spans.push(Span::styled(format!("{} ", tab.key), key));
        spans.push(Span::styled(tab.label.to_string(), label));
        if let Some(b) = tab.badge {
            spans.push(Span::styled(format!(" {b}"), badge));
        }
        spans.push(Span::styled(" ", pad));
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::widgets::Paragraph;

    fn session_views() -> [SubTab<'static>; 5] {
        let tab = |key, label, badge| SubTab { key, label, badge };
        [
            tab("d", "Detail", None),
            tab("f", "Output", None),
            tab("l", "Log", None),
            tab("v", "Gates", Some("✗")),
            tab("e", "Events", Some("12")),
        ]
    }

    fn text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn sub_tabs_text_has_keys_labels_and_badges() {
        let line = sub_tabs(&session_views(), 1, &Theme::dark());
        assert_eq!(
            text(&line),
            "  d Detail   f Output   l Log   v Gates ✗   e Events 12  "
        );
    }

    #[test]
    fn sub_tabs_active_tab_is_reversed_and_bold() {
        let line = sub_tabs(&session_views(), 1, &Theme::no_color());
        let active: Vec<_> = line
            .spans
            .iter()
            .filter(|s| s.style.add_modifier.contains(Modifier::REVERSED))
            .collect();
        let active_text: String = active.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(active_text, " f Output ");
        assert!(
            active
                .iter()
                .any(|s| s.style.add_modifier.contains(Modifier::BOLD)),
            "active label is bold"
        );
    }

    #[test]
    fn sub_tabs_out_of_range_active_reverses_nothing() {
        let line = sub_tabs(&session_views(), 9, &Theme::dark());
        assert!(
            line.spans
                .iter()
                .all(|s| !s.style.add_modifier.contains(Modifier::REVERSED))
        );
    }

    #[test]
    fn sub_tabs_badges() {
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(80, 1)).unwrap();
        let line = sub_tabs(&session_views(), 3, &theme);
        terminal
            .draw(|f| f.render_widget(Paragraph::new(line), f.area()))
            .unwrap();
        assert_snapshot!(format!("{:?}", terminal.backend().buffer()));
    }
}
