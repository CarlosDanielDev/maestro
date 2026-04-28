use crate::tui::app::TuiMode;

/// A menu entry on the Landing screen. Pairs a label, a one-letter shortcut,
/// and the `TuiMode` it pushes when activated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LandingMenuItem {
    pub label: &'static str,
    pub shortcut: char,
    pub target: LandingTarget,
}

/// What activating a Landing menu entry does. Most items push a `TuiMode`,
/// but `Quit` routes through `ConfirmExit` so menu-Enter matches the global
/// `q` handler — see `home::handle_input` for the same pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandingTarget {
    Push(TuiMode),
}

impl LandingMenuItem {
    pub const fn push(label: &'static str, shortcut: char, mode: TuiMode) -> Self {
        Self {
            label,
            shortcut,
            target: LandingTarget::Push(mode),
        }
    }
}

/// The entries shown on the Landing screen. Order is the visible order.
pub const MENU_ITEMS: &[LandingMenuItem] = &[
    LandingMenuItem::push("Dashboard", 'd', TuiMode::Dashboard),
    LandingMenuItem::push("Create Issue", 'i', TuiMode::IssueWizard),
    LandingMenuItem::push("Create Milestone", 'm', TuiMode::MilestoneWizard),
    LandingMenuItem::push("Project Stats", 's', TuiMode::ProjectStats),
    LandingMenuItem::push("PRD", 'p', TuiMode::Prd),
    LandingMenuItem::push("Roadmap", 'r', TuiMode::Roadmap),
    LandingMenuItem::push("Milestone Review", 'h', TuiMode::MilestoneHealth),
    LandingMenuItem::push("Quit", 'q', TuiMode::ConfirmExit),
];
