use ratatui::style::Color;

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

pub fn agent_color(agent_id: Option<&str>) -> Color {
    match agent_kind_for_id(agent_id) {
        AgentVisualKind::Claude => Color::LightYellow,
        AgentVisualKind::Codex => Color::LightCyan,
        AgentVisualKind::Qwen => Color::LightMagenta,
        AgentVisualKind::Opencode => Color::LightGreen,
        AgentVisualKind::Ollama => Color::Green,
        AgentVisualKind::Minimax => Color::LightBlue,
        AgentVisualKind::Unknown => Color::Gray,
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

    #[test]
    fn blank_agent_label_defaults_to_claude() {
        assert_eq!(agent_label(None), "claude");
        assert_eq!(agent_label(Some("")), "claude");
    }
}
