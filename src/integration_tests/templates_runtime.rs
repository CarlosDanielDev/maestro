//! Integration tests for HTTP-provider runtime template injection (issue #707).
//!
//! Exercises the full `SessionPool::try_promote` path: rendered cache lookup,
//! provider gating, origin gating, and TurboQuant interaction. Uses
//! `FakeRenderedStore` so no disk I/O is required.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::sync::Arc;

use crate::agent_provider::test_fakes::{FakeClaudeProvider, FakeHttpProvider};
use crate::integration_tests::helpers::{make_pool, make_session};
use crate::session::types::SessionOrigin;
use crate::templates::FakeRenderedStore;

#[test]
fn http_provider_session_appendix_contains_rendered_template_body() {
    let mut pool = make_pool(1);
    let store = FakeRenderedStore::new().with("qwen", "implement", "# Implement\n\nDo the work.");
    pool.set_rendered_template_store(Arc::new(store));
    pool.set_provider(Arc::new(FakeHttpProvider));
    pool.enqueue(make_session("fix the bug").with_active_command(Some("implement".into())));
    let ids = pool.try_promote();
    let managed = match pool.get_active_mut(ids[0]) {
        Some(m) => m,
        None => panic!("expected active managed session for {}", ids[0]),
    };
    let appendix = managed.system_prompt_appendix.as_deref().unwrap_or("");
    assert!(
        appendix.contains("# Implement\n\nDo the work."),
        "appendix should contain rendered template body, got: {appendix}"
    );
}

#[test]
fn claude_provider_session_appendix_does_not_contain_rendered_template() {
    let mut pool = make_pool(1);
    let store = FakeRenderedStore::new().with("claude", "implement", "CLAUDE_BODY");
    pool.set_rendered_template_store(Arc::new(store));
    pool.set_provider(Arc::new(FakeClaudeProvider));
    pool.enqueue(make_session("work").with_active_command(Some("implement".into())));
    let ids = pool.try_promote();
    let managed = match pool.get_active_mut(ids[0]) {
        Some(m) => m,
        None => panic!("expected active managed session for {}", ids[0]),
    };
    let appendix = managed.system_prompt_appendix.as_deref().unwrap_or("");
    assert!(
        !appendix.contains("CLAUDE_BODY"),
        "Claude provider must not get rendered template (discovers on-disk)"
    );
}

#[test]
fn session_with_no_command_produces_no_template_appendix_regression() {
    let mut pool = make_pool(1);
    let store = FakeRenderedStore::new().with("qwen", "implement", "BODY");
    pool.set_rendered_template_store(Arc::new(store));
    pool.set_provider(Arc::new(FakeHttpProvider));
    pool.enqueue(make_session("ad-hoc prompt"));
    let ids = pool.try_promote();
    let managed = match pool.get_active_mut(ids[0]) {
        Some(m) => m,
        None => panic!("expected active managed session for {}", ids[0]),
    };
    assert!(
        managed.system_prompt_appendix.is_none(),
        "ad-hoc session without active_command must keep today's behavior (no appendix)"
    );
}

#[test]
fn pool_skips_injection_when_origin_is_orchestrator_l1_integration() {
    let mut pool = make_pool(1);
    let store = FakeRenderedStore::new().with("qwen", "implement", "L1_FORBIDDEN_BODY");
    pool.set_rendered_template_store(Arc::new(store));
    pool.set_provider(Arc::new(FakeHttpProvider));
    pool.enqueue(
        make_session("work")
            .with_active_command(Some("implement".into()))
            .with_origin(SessionOrigin::OrchestratorL1),
    );
    let ids = pool.try_promote();
    let managed = match pool.get_active_mut(ids[0]) {
        Some(m) => m,
        None => panic!("expected active managed session for {}", ids[0]),
    };
    let appendix = managed.system_prompt_appendix.as_deref().unwrap_or("");
    assert!(
        !appendix.contains("L1_FORBIDDEN_BODY"),
        "L1-origin session must not receive rendered template, got: {appendix}"
    );
}

#[test]
fn turboquant_still_fires_after_template_injection() {
    use crate::turboquant::adapter::TurboQuantAdapter;

    let mut pool = make_pool(1);
    let store = FakeRenderedStore::new().with(
        "qwen",
        "implement",
        "# Implement Command\n\nFollow TDD: red, green, refactor.",
    );
    pool.set_rendered_template_store(Arc::new(store));
    pool.set_provider(Arc::new(FakeHttpProvider));
    pool.set_guardrail_prompt(
        "Guardrail: never modify auth code without explicit approval.".into(),
    );
    pool.set_turboquant_adapter(Arc::new(TurboQuantAdapter::new(4)), 4096);
    pool.enqueue(make_session("work").with_active_command(Some("implement".into())));
    let ids = pool.try_promote();
    let managed = match pool.get_active_mut(ids[0]) {
        Some(m) => m,
        None => panic!("expected active managed session for {}", ids[0]),
    };
    let appendix = managed.system_prompt_appendix.as_deref().unwrap_or("");
    assert!(
        appendix.contains("Guardrail"),
        "guardrail must survive compaction: {appendix}"
    );
    assert!(
        appendix.contains("Implement Command"),
        "template must survive compaction: {appendix}"
    );
}
