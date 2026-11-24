//! Shared fakes / builders for team_wizard unit + snapshot tests.

#![allow(dead_code)]

use crate::agent_provider::types::{AgentHealthCheck, AgentProviderId};
use crate::orchestration::dag::{IssueMeta, IssueState};
use crate::orchestration::team::{ResolvedTeam, RoleBinding, SourceTier};
use crate::orchestration::types::{Primitive, TeamRole};
use std::collections::HashMap;

pub fn make_test_team(
    name: &str,
    primitive: Primitive,
    bindings: &[(TeamRole, &str)],
    tier: SourceTier,
) -> ResolvedTeam {
    let mut b = HashMap::new();
    for (role, agent) in bindings {
        b.insert(
            *role,
            RoleBinding {
                agent: (*agent).into(),
                ..Default::default()
            },
        );
    }
    ResolvedTeam {
        name: name.into(),
        primitive,
        min_agents: vec!["claude".into()],
        bindings: b,
        source_tier: tier,
    }
}

pub fn make_health_check(agent_id: &str, available: bool) -> AgentHealthCheck {
    AgentHealthCheck {
        provider_id: AgentProviderId::new(agent_id),
        available,
        version: if available {
            Some("1.0.0".into())
        } else {
            None
        },
        message: if available {
            "ok".into()
        } else {
            "not installed".into()
        },
    }
}

pub fn make_issue_meta(
    number: u64,
    state: IssueState,
    milestone: Option<u64>,
    blocked_by: &[u64],
) -> IssueMeta {
    IssueMeta {
        number,
        state,
        milestone,
        blocked_by: blocked_by.to_vec(),
    }
}

/// #1286: an app on Compose › Review & save, the name field focused (a text
/// surface for `input_handler_text_input_tests`; keys reach the canonical
/// `Input` through `widgets::input::feed`).
pub fn compose_name_app() -> crate::tui::app::App {
    use super::{ComposeStep, TeamWizardMode, TeamWizardScreen};
    let mut app = crate::tui::make_test_app("text-input-keys");
    app.tui_mode = crate::tui::app::TuiMode::TeamWizard;
    let mut screen = TeamWizardScreen::new(Default::default());
    screen.switch_mode(TeamWizardMode::Compose);
    screen.set_compose_step_for_test(ComposeStep::Save);
    app.screen_state.team_wizard_screen = Some(screen);
    app
}

/// #1290: an app on Launch › Input with the find-issue field focused (a
/// text surface for `input_handler_text_input_tests`).
pub fn launch_find_app() -> crate::tui::app::App {
    use super::{TeamWizardMode, TeamWizardScreen};
    let mut app = crate::tui::make_test_app("text-input-keys");
    app.tui_mode = crate::tui::app::TuiMode::TeamWizard;
    let mut screen = TeamWizardScreen::new(Default::default());
    screen.switch_mode(TeamWizardMode::Launch);
    screen.set_launch_manual_issue_input_for_test("");
    app.screen_state.team_wizard_screen = Some(screen);
    app
}
