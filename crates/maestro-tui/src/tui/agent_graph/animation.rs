//! Per-tick animation state for the agent-graph renderer.
//!
//! All animations are stateless: each renders entirely from `(tick,
//! session_state)`. The flash counter is the one piece of state we read,
//! and it lives on `Session::transition_flash_remaining` (set by
//! `Session::transition_to`, decremented elsewhere by the render pump).

use std::path::Path;

use ratatui::style::{Color, Modifier};
use uuid::Uuid;

use super::model::{GraphEdge, NodeId};
use crate::session::types::{Session, SessionStatus};
use crate::tui::spinner::{AnimationPhase, animation_phase};

/// Animation-relevant fields cloned out of `Session` for the render closure.
///
/// `Canvas::paint` requires a `'static` closure, so we capture this struct
/// by move rather than holding `&Session` across the boundary.
#[derive(Clone)]
pub(super) struct SessionRenderInfo {
    pub(super) id: Uuid,
    pub(super) status: SessionStatus,
    pub(super) role: crate::session::role::Role,
    is_thinking: bool,
    current_activity: String,
    files_touched: Vec<String>,
    transition_flash_remaining: u8,
}

impl SessionRenderInfo {
    pub(super) fn from_session(s: &Session) -> Self {
        Self {
            id: s.id,
            status: s.status,
            role: s.role,
            is_thinking: s.is_thinking,
            current_activity: s.current_activity.clone(),
            files_touched: s.files_touched.clone(),
            transition_flash_remaining: s.transition_flash_remaining,
        }
    }

    fn is_tool_use_phase(&self) -> bool {
        animation_phase(self.status, self.is_thinking, &self.current_activity)
            == AnimationPhase::ToolUse
    }

    /// Render-only check: does this session's `files_touched` include `path`?
    ///
    /// String equality after `to_string_lossy` is sufficient because the result
    /// only selects an edge color. Do NOT reuse this for access control,
    /// filesystem locks, or any other authorization decision — for those, use
    /// `Path::canonicalize` + structural comparison.
    fn touches(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        self.files_touched
            .iter()
            .any(|f| f.as_str() == path_str.as_ref())
    }
}

/// Color for an edge given the current tick and the owning agent's state.
///
/// `LightCyan` during the bright 5/6 of the pulse, `Cyan` during the dim
/// 1/6, and `DarkGray` otherwise. Pure — same `(edge, sessions, tick)`
/// always yields the same color.
pub(super) fn edge_color(edge: &GraphEdge, sessions: &[SessionRenderInfo], tick: usize) -> Color {
    let NodeId::Agent(agent_uuid) = &edge.from else {
        return Color::DarkGray;
    };
    let Some(session) = sessions.iter().find(|s| s.id == *agent_uuid) else {
        return Color::DarkGray;
    };
    if !session.is_tool_use_phase() {
        return Color::DarkGray;
    }
    let NodeId::File(target_path) = &edge.to else {
        return Color::DarkGray;
    };
    if !session.touches(target_path) {
        return Color::DarkGray;
    }
    if tick % 6 < 5 {
        Color::LightCyan
    } else {
        Color::Cyan
    }
}

/// Status-transition flash override.
///
/// When a `Completed`/`Errored` session has a non-zero
/// `transition_flash_remaining`, returns a `LightGreen`/`LightRed` color
/// with a parity-modulated `BOLD`/`BOLD|REVERSED` modifier. Otherwise
/// returns the base style unchanged. Mirrors the panel-border idiom in
/// `src/tui/panels.rs:367-386`.
pub(super) fn node_animation_style(
    session: &SessionRenderInfo,
    base_color: Color,
    base_modifier: Modifier,
) -> (Color, Modifier) {
    if session.transition_flash_remaining == 0 {
        return (base_color, base_modifier);
    }
    let flash_color = match session.status {
        SessionStatus::Completed => Color::LightGreen,
        SessionStatus::Errored => Color::LightRed,
        _ => return (base_color, base_modifier),
    };
    let flash_mod = if session.transition_flash_remaining.is_multiple_of(2) {
        Modifier::BOLD | Modifier::REVERSED
    } else {
        Modifier::BOLD
    };
    (flash_color, flash_mod)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn info(status: SessionStatus, activity: &str, files: &[&str]) -> SessionRenderInfo {
        SessionRenderInfo {
            id: Uuid::nil(),
            status,
            role: crate::session::role::Role::default(),
            is_thinking: false,
            current_activity: activity.to_string(),
            files_touched: files.iter().map(|f| (*f).to_string()).collect(),
            transition_flash_remaining: 0,
        }
    }

    fn edge(from_id: Uuid, file: &str) -> GraphEdge {
        GraphEdge {
            from: NodeId::Agent(from_id),
            to: NodeId::File(PathBuf::from(file)),
        }
    }

    #[test]
    fn edge_color_dark_gray_for_idle_agent() {
        let s = info(
            SessionStatus::Running,
            "Working on something",
            &["src/a.rs"],
        );
        let e = edge(s.id, "src/a.rs");
        assert_eq!(edge_color(&e, &[s], 0), Color::DarkGray);
    }

    #[test]
    fn edge_color_light_cyan_for_tool_use_at_tick_0() {
        let s = info(SessionStatus::Running, "Read: src/a.rs", &["src/a.rs"]);
        let e = edge(s.id, "src/a.rs");
        assert_eq!(edge_color(&e, &[s], 0), Color::LightCyan);
    }

    #[test]
    fn edge_color_dim_cyan_at_tick_5() {
        let s = info(SessionStatus::Running, "Read: src/a.rs", &["src/a.rs"]);
        let e = edge(s.id, "src/a.rs");
        assert_eq!(edge_color(&e, &[s], 5), Color::Cyan);
    }

    #[test]
    fn edge_color_dark_gray_when_target_file_not_touched() {
        let s = info(SessionStatus::Running, "Read: src/x.rs", &["src/y.rs"]);
        let e = edge(s.id, "src/z.rs");
        assert_eq!(edge_color(&e, &[s], 0), Color::DarkGray);
    }

    #[test]
    fn flash_returns_base_when_counter_zero() {
        let mut s = info(SessionStatus::Completed, "", &[]);
        s.transition_flash_remaining = 0;
        let (color, modifier) = node_animation_style(&s, Color::Gray, Modifier::DIM);
        assert_eq!(color, Color::Gray);
        assert_eq!(modifier, Modifier::DIM);
    }

    #[test]
    fn flash_returns_light_green_bold_reversed_for_completed_even() {
        let mut s = info(SessionStatus::Completed, "", &[]);
        s.transition_flash_remaining = 4;
        let (color, modifier) = node_animation_style(&s, Color::Gray, Modifier::DIM);
        assert_eq!(color, Color::LightGreen);
        assert_eq!(modifier, Modifier::BOLD | Modifier::REVERSED);
    }

    #[test]
    fn flash_returns_light_green_bold_only_for_completed_odd() {
        let mut s = info(SessionStatus::Completed, "", &[]);
        s.transition_flash_remaining = 3;
        let (color, modifier) = node_animation_style(&s, Color::Gray, Modifier::DIM);
        assert_eq!(color, Color::LightGreen);
        assert_eq!(modifier, Modifier::BOLD);
    }

    #[test]
    fn flash_returns_light_red_for_errored() {
        let mut s = info(SessionStatus::Errored, "", &[]);
        s.transition_flash_remaining = 2;
        let (color, modifier) = node_animation_style(&s, Color::Red, Modifier::BOLD);
        assert_eq!(color, Color::LightRed);
        assert_eq!(modifier, Modifier::BOLD | Modifier::REVERSED);
    }

    #[test]
    fn flash_does_not_apply_to_running_session() {
        let mut s = info(SessionStatus::Running, "", &[]);
        s.transition_flash_remaining = 4;
        let (color, modifier) = node_animation_style(&s, Color::Green, Modifier::BOLD);
        assert_eq!(color, Color::Green);
        assert_eq!(modifier, Modifier::BOLD);
    }

    // ── role survives Session → SessionRenderInfo projection ────────────────

    #[test]
    fn render_info_carries_role_through_from_session() {
        use crate::session::role::Role;

        let mut session = Session::new(
            "task".to_string(),
            "claude-opus-4-5".to_string(),
            "orchestrator".to_string(),
            None,
            Some(Role::Reviewer),
        );
        session.id = Uuid::nil();
        let info = SessionRenderInfo::from_session(&session);
        assert_eq!(
            info.role,
            Role::Reviewer,
            "SessionRenderInfo must carry the role through from Session so the \
             render closure can compute role color without holding &Session"
        );
    }
}
