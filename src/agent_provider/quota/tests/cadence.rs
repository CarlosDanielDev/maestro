use super::*;

#[test]
fn refresh_interval_outlasts_the_cache_ttl() {
    // A tick inside the TTL window is skipped, so the tick must come later.
    let interval = crate::agent_provider::quota::refresh_interval();
    let ttl = cache_mod::TTL.to_std().expect("positive ttl");
    assert!(
        interval > ttl,
        "interval {interval:?} must exceed ttl {ttl:?}"
    );
}
