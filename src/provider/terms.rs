use super::types::ProviderKind;

pub fn provider_milestone_label(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Github => "Milestone",
        ProviderKind::AzureDevops => "Iteration",
    }
}

pub fn provider_issue_label_plural(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Github => "Issues",
        ProviderKind::AzureDevops => "Work Items",
    }
}

pub fn provider_issue_label_lowercase(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Github => "issue",
        ProviderKind::AzureDevops => "work item",
    }
}

pub fn provider_display_name(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Github => "GitHub",
        ProviderKind::AzureDevops => "Azure DevOps",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_milestone_label_matches_provider_terms() {
        assert_eq!(provider_milestone_label(ProviderKind::Github), "Milestone");
        assert_eq!(
            provider_milestone_label(ProviderKind::AzureDevops),
            "Iteration"
        );
    }

    #[test]
    fn provider_labels_match_provider_terms() {
        assert_eq!(provider_issue_label_plural(ProviderKind::Github), "Issues");
        assert_eq!(
            provider_issue_label_plural(ProviderKind::AzureDevops),
            "Work Items"
        );
        assert_eq!(
            provider_issue_label_lowercase(ProviderKind::Github),
            "issue"
        );
        assert_eq!(
            provider_issue_label_lowercase(ProviderKind::AzureDevops),
            "work item"
        );
        assert_eq!(provider_display_name(ProviderKind::Github), "GitHub");
        assert_eq!(
            provider_display_name(ProviderKind::AzureDevops),
            "Azure DevOps"
        );
    }
}
