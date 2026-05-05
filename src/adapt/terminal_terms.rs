use crate::provider::types::{
    ProviderKind, provider_display_name, provider_issue_label_lowercase,
    provider_issue_label_plural, provider_milestone_label,
};

pub struct AdaptTerminalTerms {
    pub milestone_label: &'static str,
    pub milestone_label_plural: String,
    pub milestone_label_lowercase: String,
    pub milestone_label_plural_lowercase: String,
    pub issue_label_plural: &'static str,
    pub issue_label_plural_lowercase: String,
    pub issue_label_lowercase: &'static str,
    pub provider_name: &'static str,
}

impl AdaptTerminalTerms {
    pub fn from_provider_kind(kind: ProviderKind) -> Self {
        let milestone_label = provider_milestone_label(kind);
        let milestone_label_plural = format!("{milestone_label}s");
        Self {
            milestone_label,
            milestone_label_lowercase: milestone_label.to_ascii_lowercase(),
            milestone_label_plural_lowercase: milestone_label_plural.to_ascii_lowercase(),
            milestone_label_plural,
            issue_label_plural: provider_issue_label_plural(kind),
            issue_label_plural_lowercase: provider_issue_label_plural(kind).to_ascii_lowercase(),
            issue_label_lowercase: provider_issue_label_lowercase(kind),
            provider_name: provider_display_name(kind),
        }
    }
}
