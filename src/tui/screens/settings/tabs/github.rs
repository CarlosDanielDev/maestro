use crate::config::Config;
use crate::tui::widgets::{Dropdown, ListEditor, NumberStepper, Toggle, WidgetKind};

use super::field;
use crate::tui::screens::settings::SettingsField;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    let g = &config.github;
    let merge_options: Vec<String> = vec!["merge", "squash", "rebase"]
        .into_iter()
        .map(String::from)
        .collect();
    let merge_idx = match g.merge_method {
        crate::config::MergeMethod::Merge => 0,
        crate::config::MergeMethod::Squash => 1,
        crate::config::MergeMethod::Rebase => 2,
    };
    vec![
        field(WidgetKind::ListEditor(ListEditor::new(
            "issue_filter_labels",
            g.issue_filter_labels.clone(),
        ))),
        field(WidgetKind::Toggle(Toggle::new("auto_pr", g.auto_pr))),
        field(WidgetKind::NumberStepper(
            NumberStepper::new("cache_ttl_secs", g.cache_ttl_secs as i64, 30, 3600).with_step(30),
        )),
        field(WidgetKind::Toggle(Toggle::new("auto_merge", g.auto_merge))),
        field(WidgetKind::Dropdown(Dropdown::new(
            "merge_method",
            merge_options,
            merge_idx,
        ))),
    ]
}
