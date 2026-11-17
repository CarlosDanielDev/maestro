#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentVisualKind {
    Claude,
    Codex,
    Qwen,
    Opencode,
    Ollama,
    Minimax,
    Unknown,
}

pub fn agent_kind_for_id(agent_id: Option<&str>) -> AgentVisualKind {
    let id = agent_id.unwrap_or("claude").to_ascii_lowercase();
    if id.contains("codex") {
        AgentVisualKind::Codex
    } else if id.contains("qwen") {
        AgentVisualKind::Qwen
    } else if id.contains("opencode") || id.contains("open-code") {
        AgentVisualKind::Opencode
    } else if id.contains("ollama") {
        AgentVisualKind::Ollama
    } else if id.contains("minimax") {
        AgentVisualKind::Minimax
    } else if id.contains("claude") {
        AgentVisualKind::Claude
    } else {
        AgentVisualKind::Unknown
    }
}

pub fn agent_label(agent_id: Option<&str>) -> String {
    agent_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or("claude")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn maps_known_provider_ids_to_visual_kinds() {
        assert_eq!(agent_kind_for_id(Some("claude")), AgentVisualKind::Claude);
        assert_eq!(agent_kind_for_id(Some("codex")), AgentVisualKind::Codex);
        assert_eq!(agent_kind_for_id(Some("qwen")), AgentVisualKind::Qwen);
        assert_eq!(
            agent_kind_for_id(Some("opencode")),
            AgentVisualKind::Opencode
        );
        assert_eq!(agent_kind_for_id(Some("ollama")), AgentVisualKind::Ollama);
        assert_eq!(agent_kind_for_id(Some("minimax")), AgentVisualKind::Minimax);
        assert_eq!(
            agent_kind_for_id(Some("future-agent")),
            AgentVisualKind::Unknown
        );
    }

    const ALL_IDS: [&str; 7] = [
        "claude",
        "codex",
        "qwen",
        "opencode",
        "ollama",
        "minimax",
        "future-agent",
    ];

    #[test]
    fn no_color_theme_draws_every_agent_badge_as_reset() {
        let theme = crate::tui::theme::Theme::no_color();
        for id in ALL_IDS {
            assert_eq!(theme.agent_color(Some(id)), Color::Reset, "agent {id}");
        }
        assert_eq!(theme.agent_color(None), Color::Reset);
    }

    #[test]
    fn coloured_presets_keep_todays_agent_colours() {
        use crate::tui::theme::Theme;
        let expected = [
            Color::LightYellow,
            Color::LightCyan,
            Color::LightMagenta,
            Color::LightGreen,
            Color::Green,
            Color::LightBlue,
            Color::Gray,
        ];
        for theme in [
            Theme::dark(),
            Theme::light(),
            Theme::retro(),
            Theme::high_contrast(),
        ] {
            for (id, colour) in ALL_IDS.into_iter().zip(expected) {
                assert_eq!(theme.agent_color(Some(id)), colour, "agent {id}");
            }
            assert_eq!(theme.agent_color(None), Color::LightYellow);
        }
    }

    #[test]
    fn blank_agent_label_defaults_to_claude() {
        assert_eq!(agent_label(None), "claude");
        assert_eq!(agent_label(Some("")), "claude");
    }
}
