#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    Home,
    Run,
    Plan,
    Review,
    Insights,
    System,
}

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
        assert_eq!(ToolId::RunSessions.bucket(), Bucket::Run);
        assert_eq!(ToolId::PlanIssues.bucket(), Bucket::Plan);
        assert_eq!(ToolId::ReviewPrs.bucket(), Bucket::Review);
        assert_eq!(ToolId::InsightsCost.bucket(), Bucket::Insights);
        assert_eq!(ToolId::SystemSettings.bucket(), Bucket::System);
        assert_eq!(ToolId::Home.bucket(), Bucket::Home);
    }

    #[test]
    fn select_tool_updates_bucket() {
        let mut s = SidebarState::default();
        s.select(ToolId::InsightsCost);
        assert_eq!(s.active_bucket, Bucket::Insights);
        assert_eq!(s.active_tool, ToolId::InsightsCost);
    }
}
