use std::borrow::Cow;

use crate::tui::theme::Theme;
use crate::tui::widgets::BrailleSpinner;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmptyStateMode {
    Idle,
    Loading { tick: usize },
}

/// Shared action-oriented placeholder for empty and loading TUI panels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyState<'a> {
    title: Cow<'a, str>,
    message: Cow<'a, str>,
    hint: Option<Cow<'a, str>>,
    mode: EmptyStateMode,
}

impl<'a> EmptyState<'a> {
    pub fn idle(
        title: impl Into<Cow<'a, str>>,
        message: impl Into<Cow<'a, str>>,
        hint: impl Into<Cow<'a, str>>,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            hint: Some(hint.into()),
            mode: EmptyStateMode::Idle,
        }
    }

    pub fn loading(
        title: impl Into<Cow<'a, str>>,
        message: impl Into<Cow<'a, str>>,
        tick: usize,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            hint: None,
            mode: EmptyStateMode::Loading { tick },
        }
    }

    pub fn render(self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block(&self.title, false);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut lines = vec![self.primary_line(theme)];
        if let Some(hint) = self.hint {
            lines.push(Line::from(Span::styled(
                hint,
                Style::default()
                    .fg(theme.text_secondary)
                    .add_modifier(Modifier::DIM),
            )));
        }

        let content_height = lines.len() as u16;
        let top_padding = inner.height.saturating_sub(content_height) / 2;
        let content_area = Rect {
            y: inner.y.saturating_add(top_padding),
            height: content_height.min(inner.height),
            ..inner
        };

        f.render_widget(
            Paragraph::new(lines).alignment(Alignment::Center),
            content_area,
        );
    }

    fn primary_line(&self, theme: &Theme) -> Line<'a> {
        match self.mode {
            EmptyStateMode::Idle => Line::from(Span::styled(
                self.message.clone(),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            )),
            EmptyStateMode::Loading { tick } => {
                BrailleSpinner::render(tick, self.message.clone(), true, theme)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use ratatui::{Terminal, backend::TestBackend};

    fn render_state(state: EmptyState<'_>) -> anyhow::Result<String> {
        let mut terminal = Terminal::new(TestBackend::new(40, 8))?;
        let theme = Theme::dark();

        terminal.draw(|f| {
            state.render(f, f.area(), &theme);
        })?;

        Ok(terminal.backend().to_string())
    }

    #[test]
    fn idle_renders_title_message_and_hint() -> anyhow::Result<()> {
        assert_snapshot!(render_state(EmptyState::idle(
            "Recent Activity",
            "No recent sessions.",
            "Press [r] to launch one.",
        ))?);
        Ok(())
    }

    #[test]
    fn loading_renders_fixed_braille_tick() -> anyhow::Result<()> {
        let output = render_state(EmptyState::loading("Roadmap", "Fetching milestones", 3))?;

        assert!(
            output.contains("⠸ Fetching milestones"),
            "loading state should render the tick 3 braille frame:\n{output}"
        );
        assert_snapshot!(output);
        Ok(())
    }
}
