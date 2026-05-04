use super::*;

#[test]
fn first_origin_remote_url_prefers_fetch_url() {
    let remote = "upstream\thttps://example.com/upstream.git (fetch)\n\
                  origin\tgit@github.com:owner/repo.git (push)\n\
                  origin\thttps://github.com/owner/repo.git (fetch)\n";
    assert_eq!(
        first_origin_remote_url(remote).as_deref(),
        Some("https://github.com/owner/repo.git")
    );
}

#[test]
fn validate_azure_devops_org_accepts_supported_forms() {
    assert!(validate_azure_devops_organization_url(
        "https://dev.azure.com/MyOrg"
    ));
    assert!(validate_azure_devops_organization_url(
        "https://MyOrg.visualstudio.com"
    ));
}

#[test]
fn validate_azure_devops_org_rejects_invalid_forms() {
    for input in [
        "http://dev.azure.com/MyOrg",
        "https://dev.azure.com/",
        "https://dev.azure.com/MyOrg/Project",
        "https://MyOrg@dev.azure.com/MyOrg",
        "https://example.com/MyOrg",
        "dev.azure.com/MyOrg",
        "",
    ] {
        assert!(
            !validate_azure_devops_organization_url(input),
            "accepted {input:?}"
        );
    }
}
