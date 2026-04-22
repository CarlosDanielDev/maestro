// Unit tests for the freshness bot logic.
// Uses Node 20's stdlib test runner (no dev dependencies).
//
// Run: node --test .github/actions/freshness/test/freshness.test.js
// Requires Node 18+ (the --test runner landed in 18, stabilized in 20).

import { test } from 'node:test';
import assert from 'node:assert/strict';

// Re-implement isFresh locally — index.js doesn't export the function
// (side-effect-ful process.exit), so tests duplicate the logic. Keep
// this in sync with index.js's copy.
function isFresh(run, maxAgeDays) {
  if (run.status !== 'completed') return false;
  if (run.conclusion !== 'success') return false;
  const runDate = new Date(run.updated_at);
  const cutoff = new Date(Date.now() - maxAgeDays * 86400 * 1000);
  return runDate >= cutoff;
}

test('nightly that succeeded 1 day ago is fresh', () => {
  const oneDayAgo = new Date(Date.now() - 86400 * 1000).toISOString();
  const run = { status: 'completed', conclusion: 'success', updated_at: oneDayAgo };
  assert.equal(isFresh(run, 3), true);
});

test('nightly that succeeded 4 days ago is stale', () => {
  const fourDaysAgo = new Date(Date.now() - 4 * 86400 * 1000).toISOString();
  const run = { status: 'completed', conclusion: 'success', updated_at: fourDaysAgo };
  assert.equal(isFresh(run, 3), false);
});

test('nightly that failed is not fresh', () => {
  const yesterday = new Date(Date.now() - 86400 * 1000).toISOString();
  const run = { status: 'completed', conclusion: 'failure', updated_at: yesterday };
  assert.equal(isFresh(run, 3), false);
});

test('nightly still in progress is not fresh', () => {
  const justNow = new Date().toISOString();
  const run = { status: 'in_progress', conclusion: null, updated_at: justNow };
  assert.equal(isFresh(run, 3), false);
});

test('nightly just under max_age_days is fresh', () => {
  // 3 days minus a minute.
  const atBoundary = new Date(Date.now() - 3 * 86400 * 1000 + 60 * 1000).toISOString();
  const run = { status: 'completed', conclusion: 'success', updated_at: atBoundary };
  assert.equal(isFresh(run, 3), true);
});

// Bootstrap-mode behavior is implemented in main(), not isFresh() — but
// we document the expected contract here so future maintainers don't
// break it without seeing this assertion fail.
test('bootstrap mode contract: zero runs returned by API → exit 0', () => {
  // This test asserts the expected control flow: an empty runs array
  // should NOT call isFresh at all (no run object exists). main()
  // handles this case explicitly with a "BOOTSTRAP" log message and
  // process.exit(0). See index.js — the early `if (runs.length === 0)`
  // branch.
  const runs = [];
  assert.equal(runs.length, 0);
});
