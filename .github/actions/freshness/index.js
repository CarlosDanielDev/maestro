// Freshness bot: queries the GitHub API for the most recent scheduled
// run of the nightly workflow on `main`, asserts it succeeded within
// the configured max_age_days.
//
// Exits with status 0 on success, 1 on stale/failure.
//
// Uses Node 20 stdlib only (no npm deps). Pure REST calls via fetch.

const GITHUB_API = 'https://api.github.com';

async function fetchWorkflowRuns({ owner, repo, workflowFile, token }) {
  const url = `${GITHUB_API}/repos/${owner}/${repo}/actions/workflows/${workflowFile}/runs?branch=main&event=schedule&per_page=10`;
  const res = await fetch(url, {
    headers: {
      'Authorization': `Bearer ${token}`,
      'Accept': 'application/vnd.github+json',
      'X-GitHub-Api-Version': '2022-11-28',
    },
  });
  if (!res.ok) {
    throw new Error(`GitHub API error ${res.status}: ${await res.text()}`);
  }
  return (await res.json()).workflow_runs;
}

function isFresh(run, maxAgeDays) {
  if (run.status !== 'completed') return false;
  if (run.conclusion !== 'success') return false;
  const runDate = new Date(run.updated_at);
  const cutoff = new Date(Date.now() - maxAgeDays * 86400 * 1000);
  return runDate >= cutoff;
}

async function main() {
  const repoFull = process.env.GITHUB_REPOSITORY ?? '';
  const [owner, repo] = repoFull.split('/');
  if (!owner || !repo) {
    console.error('Error: GITHUB_REPOSITORY env var missing or malformed.');
    process.exit(1);
  }
  const token = process.env.INPUT_GITHUB_TOKEN;
  if (!token) {
    console.error('Error: INPUT_GITHUB_TOKEN is not set.');
    process.exit(1);
  }
  const maxAgeDays = parseInt(process.env.INPUT_MAX_AGE_DAYS || '3', 10);
  const workflowFile = process.env.INPUT_WORKFLOW_FILE || 'nightly.yml';

  const runs = await fetchWorkflowRuns({ owner, repo, workflowFile, token });
  if (runs.length === 0) {
    console.log('No scheduled nightly runs found on main.');
    process.exit(1);
  }

  const mostRecent = runs[0];
  const fresh = isFresh(mostRecent, maxAgeDays);

  console.log(`Most recent ${workflowFile}: ${mostRecent.status} / ${mostRecent.conclusion} at ${mostRecent.updated_at}`);
  console.log(`Max age: ${maxAgeDays} days`);

  if (fresh) {
    console.log('Freshness check: PASS');
    process.exit(0);
  } else {
    console.log('Freshness check: FAIL (nightly is stale, failed, or missing)');
    process.exit(1);
  }
}

main().catch((err) => {
  console.error(`Error: ${err.message}`);
  process.exit(1);
});
