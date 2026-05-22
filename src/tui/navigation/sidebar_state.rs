// Scaffold for the sidebar redesign; consumed by the upcoming navigation
// rewrite. Allow dead_code here so the build is clean until the call sites
// land — the inline tests still exercise the scaffold for correctness.
#![allow(dead_code)]

/// Top-level sidebar bucket — `Home` is a singleton; the other five (`Run`/`Plan`/`Review`/`Insights`/`System`) group tools
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    Home,
    Run,
    Plan,
    Review,
    Insights,
    System,
}

/// Every addressable sidebar tool — `Home` singleton plus 19 bucketed tools across Run/Plan/Review/Insights/System
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolId {
    Home,
    RunSessions,
    RunInteractions,
    RunQueue,
    RunAdapt,
    RunPrompt,
    PlanIssues,
    PlanMilestones,
    PlanRoadmap,
    PlanPrd,
    ReviewPrs,
    ReviewCi,
    ReviewReleases,
    InsightsCost,
    InsightsTokens,
    InsightsTurboquant,
    InsightsAgents,
    InsightsStats,
    SystemSettings,
    SystemTeams,
}

impl ToolId {
    /// Returns the `Bucket` that owns this tool
    pub fn bucket(self) -> Bucket {
        use ToolId::*;
        match self {
            Home => Bucket::Home,
            RunSessions | RunInteractions | RunQueue | RunAdapt | RunPrompt => Bucket::Run,
            PlanIssues | PlanMilestones | PlanRoadmap | PlanPrd => Bucket::Plan,
            ReviewPrs | ReviewCi | ReviewReleases => Bucket::Review,
            InsightsCost | InsightsTokens | InsightsTurboquant | InsightsAgents | InsightsStats => {
                Bucket::Insights
            }
            SystemSettings | SystemTeams => Bucket::System,
        }
    }
}

/// Runtime state for the sidebar — tracks the active bucket and tool and whether the panel is collapsed
#[derive(Debug, Clone)]
pub struct SidebarState {
    pub active_bucket: Bucket,
    pub active_tool: ToolId,
    pub collapsed: bool,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            active_bucket: Bucket::Home,
            active_tool: ToolId::Home,
            collapsed: false,
        }
    }
}

impl SidebarState {
    /// Activates `tool` and syncs `active_bucket` to its owning bucket
    pub fn select(&mut self, tool: ToolId) {
        self.active_tool = tool;
        self.active_bucket = tool.bucket();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_state_default_lands_home() {
        let s = SidebarState::default();
        assert_eq!(s.active_bucket, Bucket::Home);
        assert_eq!(s.active_tool, ToolId::Home);
        assert!(!s.collapsed);
    }

    #[test]
    fn tool_id_bucket_is_consistent() {
        use ToolId::*;
        let cases: &[(ToolId, Bucket)] = &[
            (Home, Bucket::Home),
            (RunSessions, Bucket::Run),
            (RunInteractions, Bucket::Run),
            (RunQueue, Bucket::Run),
            (RunAdapt, Bucket::Run),
            (RunPrompt, Bucket::Run),
            (PlanIssues, Bucket::Plan),
            (PlanMilestones, Bucket::Plan),
            (PlanRoadmap, Bucket::Plan),
            (PlanPrd, Bucket::Plan),
            (ReviewPrs, Bucket::Review),
            (ReviewCi, Bucket::Review),
            (ReviewReleases, Bucket::Review),
            (InsightsCost, Bucket::Insights),
            (InsightsTokens, Bucket::Insights),
            (InsightsTurboquant, Bucket::Insights),
            (InsightsAgents, Bucket::Insights),
            (InsightsStats, Bucket::Insights),
            (SystemSettings, Bucket::System),
            (SystemTeams, Bucket::System),
        ];
        assert_eq!(cases.len(), 20);
        for (tool, expected) in cases {
            assert_eq!(tool.bucket(), *expected, "bucket mismatch for {:?}", tool);
        }
    }

    #[test]
    fn select_tool_updates_bucket() {
        let mut s = SidebarState::default();
        s.select(ToolId::InsightsCost);
        assert_eq!(s.active_bucket, Bucket::Insights);
        assert_eq!(s.active_tool, ToolId::InsightsCost);
    }
}
