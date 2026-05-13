//! Tests for `template.rs`. Extracted into a sibling file via `#[path]` so
//! `template.rs` itself stays under the repo's 400-line file-size guardrail.

use super::*;
use crate::init::DetectedStack;

#[test]
fn stack_defaults_rust() {
    let d = StackDefaults::for_stack(DetectedStack::Rust);
    assert_eq!(d.build_command, "cargo build");
    assert_eq!(d.test_command, "cargo test");
    assert_eq!(d.run_command, "cargo run");
}

#[test]
fn stack_defaults_node() {
    let d = StackDefaults::for_stack(DetectedStack::Node);
    assert_eq!(d.build_command, "npm run build");
    assert_eq!(d.test_command, "npm test");
    assert_eq!(d.run_command, "npm start");
}

#[test]
fn stack_defaults_python() {
    let d = StackDefaults::for_stack(DetectedStack::Python);
    assert_eq!(d.build_command, "python -m build");
    assert_eq!(d.test_command, "pytest");
    assert_eq!(d.run_command, "python main.py");
}

#[test]
fn stack_defaults_go() {
    let d = StackDefaults::for_stack(DetectedStack::Go);
    assert_eq!(d.build_command, "go build ./...");
    assert_eq!(d.test_command, "go test ./...");
    assert_eq!(d.run_command, "go run .");
}

#[test]
fn template_render_rust_contains_cargo_test() {
    let out = render(&[DetectedStack::Rust]);
    assert!(out.contains("language = \"rust\""), "{out}");
    assert!(out.contains("build_command = \"cargo build\""), "{out}");
    assert!(out.contains("test_command = \"cargo test\""), "{out}");
    assert!(out.contains("run_command = \"cargo run\""), "{out}");
}

#[test]
fn template_render_node_contains_npm_test() {
    let out = render(&[DetectedStack::Node]);
    assert!(out.contains("language = \"node\""), "{out}");
    assert!(out.contains("build_command = \"npm run build\""), "{out}");
    assert!(out.contains("test_command = \"npm test\""), "{out}");
    assert!(out.contains("run_command = \"npm start\""), "{out}");
}

#[test]
fn template_render_python_contains_pytest() {
    let out = render(&[DetectedStack::Python]);
    assert!(out.contains("language = \"python\""), "{out}");
    assert!(out.contains("test_command = \"pytest\""), "{out}");
}

#[test]
fn template_render_go_contains_go_test() {
    let out = render(&[DetectedStack::Go]);
    assert!(out.contains("language = \"go\""), "{out}");
    assert!(out.contains("test_command = \"go test ./...\""), "{out}");
}

#[test]
fn template_render_polyglot_lists_languages() {
    let out = render(&[DetectedStack::Rust, DetectedStack::Node]);
    assert!(out.contains("language = \"rust\""), "{out}");
    assert!(out.contains("languages = ["), "{out}");
    assert!(out.contains("\"rust\""), "{out}");
    assert!(out.contains("\"node\""), "{out}");
    assert!(out.contains("build_command = \"cargo build\""), "{out}");
}

#[test]
fn template_render_gates_test_command_matches_primary_stack() {
    let out = render(&[DetectedStack::Node]);
    let gates_idx = out.find("[gates]").expect("gates section");
    let after_gates = &out[gates_idx..];
    assert!(
        after_gates.contains("test_command = \"npm test\""),
        "expected gates.test_command = npm test in:\n{after_gates}"
    );
}

#[test]
fn template_render_omits_experimental_section() {
    let out = render(&[DetectedStack::Rust]);
    assert!(!out.contains("[experimental]"), "{out}");
    assert!(!out.contains("azure_devops"), "{out}");
}

#[test]
fn template_render_includes_github_provider_section() {
    let out = render(&[DetectedStack::Rust]);
    assert!(out.contains("[provider]"), "{out}");
    assert!(out.contains("kind = \"github\""), "{out}");
}

#[test]
fn template_render_azure_devops_provider_omits_experimental_opt_in() {
    let out = render_with_provider(
        &[DetectedStack::Rust],
        &ProviderTemplate::azure_devops("https://dev.azure.com/MyOrg".into(), "MyProject".into()),
    );
    assert!(out.contains("kind = \"azure_devops\""), "{out}");
    assert!(
        out.contains("organization = \"https://dev.azure.com/MyOrg\""),
        "{out}"
    );
    assert!(out.contains("az_project = \"MyProject\""), "{out}");
    assert!(!out.contains("[experimental]"), "{out}");
    assert!(!out.contains("azure_devops = true"), "{out}");
}

#[test]
fn template_render_empty_stacks_produces_generic_template() {
    let out = render(&[]);
    assert!(!out.contains("\"cargo build\""), "{out}");
    assert!(!out.contains("\"npm run build\""), "{out}");
    assert!(!out.contains("\"go build ./...\""), "{out}");
    assert!(!out.contains("\"pytest\""), "{out}");
    assert!(out.contains("# "), "{out}");
    assert!(out.contains("build_command"), "{out}");
}

#[test]
fn template_render_contains_views_section_with_agent_graph_enabled() {
    let out = render(&[DetectedStack::Rust]);
    assert!(
        out.contains("[views]"),
        "template must emit a [views] section: {out}"
    );
    assert!(
        out.contains("agent_graph_enabled = true"),
        "template must default agent_graph_enabled = true: {out}"
    );
}
