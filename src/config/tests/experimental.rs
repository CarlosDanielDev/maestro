use super::*;

#[test]
fn experimental_config_default_disables_azure_devops() {
    assert!(!ExperimentalConfig::default().azure_devops);
}

#[test]
fn config_validate_rejects_azure_devops_without_experimental_opt_in() {
    let toml_str = format!(
        r#"{MINIMAL_TOML}
[provider]
kind = "azure_devops"
"#
    );
    let cfg: Config = match toml::from_str(&toml_str) {
        Ok(cfg) => cfg,
        Err(err) => panic!("azure devops config must parse: {err}"),
    };
    let err = match cfg.validate() {
        Ok(()) => panic!("azure devops must require opt-in"),
        Err(err) => err,
    };
    assert_eq!(err.to_string(), Config::AZURE_DEVOPS_EXPERIMENTAL_ERROR);
    assert!(
        err.to_string()
            .contains("[experimental] azure_devops = true")
    );
}

#[test]
fn config_validate_accepts_azure_devops_with_experimental_opt_in() {
    let toml_str = format!(
        r#"{MINIMAL_TOML}
[provider]
kind = "azure_devops"
organization = "https://dev.azure.com/MyOrg"
az_project = "MyProject"

[experimental]
azure_devops = true
"#
    );
    let cfg: Config = match toml::from_str(&toml_str) {
        Ok(cfg) => cfg,
        Err(err) => panic!("azure devops opt-in config must parse: {err}"),
    };
    if let Err(err) = cfg.validate() {
        panic!("explicit opt-in must pass: {err}");
    }
}

#[test]
fn config_validate_rejects_azure_devops_missing_fields() {
    let toml_str = format!(
        r#"{MINIMAL_TOML}
[provider]
kind = "azure_devops"

[experimental]
azure_devops = true
"#
    );
    let cfg: Config = toml::from_str(&toml_str).expect("azure devops config parses");
    let err = cfg
        .validate()
        .expect_err("azure devops fields are required");
    assert!(err.to_string().contains("provider.organization"));
}

#[test]
fn config_validate_rejects_azure_devops_invalid_organization() {
    let toml_str = format!(
        r#"{MINIMAL_TOML}
[provider]
kind = "azure_devops"
organization = "https://dev.azure.com/MyOrg/Project"
az_project = "MyProject"

[experimental]
azure_devops = true
"#
    );
    let cfg: Config = toml::from_str(&toml_str).expect("azure devops config parses");
    let err = cfg
        .validate()
        .expect_err("azure devops organization must be valid");
    assert!(err.to_string().contains("provider.organization"));
}

#[test]
fn config_validate_accepts_github_with_or_without_azure_devops_opt_in() {
    for azure_devops in [false, true] {
        let toml_str = format!(
            r#"{MINIMAL_TOML}
[provider]
kind = "github"

[experimental]
azure_devops = {azure_devops}
"#
        );
        let cfg: Config = match toml::from_str(&toml_str) {
            Ok(cfg) => cfg,
            Err(err) => panic!("github config must parse: {err}"),
        };
        if let Err(err) = cfg.validate() {
            panic!("github config must not be gated: {err}");
        }
    }
}

#[test]
fn config_without_experimental_section_defaults_to_false_and_round_trips() {
    let cfg: Config = match toml::from_str(MINIMAL_TOML) {
        Ok(cfg) => cfg,
        Err(err) => panic!("minimal config must parse: {err}"),
    };
    assert!(!cfg.experimental.azure_devops);

    let serialized = match toml::to_string_pretty(&cfg) {
        Ok(serialized) => serialized,
        Err(err) => panic!("serialize config: {err}"),
    };
    assert!(
        !serialized.contains("[experimental]"),
        "default experimental config should be omitted: {serialized}"
    );

    let reloaded: Config = match toml::from_str(&serialized) {
        Ok(reloaded) => reloaded,
        Err(err) => panic!("serialized config must parse: {err}"),
    };
    assert!(!reloaded.experimental.azure_devops);
}
