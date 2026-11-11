//! Detection and logging for the retired `experimental.azure_devops` flag.
//! Moved out of `config/mod.rs` to keep that file under the 400-line cap (#1065).

pub(super) fn has_legacy_azure_devops_flag(content: &str) -> bool {
    // Whole-document parse: callers pass full maestro.toml files, and toml 1.x
    // `Value::from_str` only accepts a bare value.
    let Ok(value) = toml::from_str::<toml::Value>(content) else {
        return false;
    };
    matches!(
        value
            .get("experimental")
            .and_then(|experimental| experimental.get("azure_devops"))
            .and_then(toml::Value::as_bool),
        Some(false)
    )
}

pub(super) fn log_legacy_azure_devops_config(azure_devops: bool) {
    if !azure_devops {
        tracing::debug!(
            "`experimental.azure_devops = false` is retained for backward compatibility; \
             Azure DevOps is stable and no startup gate is applied"
        );
    }
}
