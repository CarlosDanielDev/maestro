use crate::provider::types::MergeMethod;

pub(super) fn merge_method_args(method: MergeMethod) -> Vec<&'static str> {
    match method {
        MergeMethod::Squash => vec!["--squash", "true"],
        MergeMethod::Rebase => vec!["--merge-strategy", "rebase"],
        MergeMethod::Merge => vec!["--merge-strategy", "noFastForward"],
    }
}
