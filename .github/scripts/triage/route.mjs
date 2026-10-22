import { appendFileSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const retryLabels = new Set([
    'triage: needs triage', 'triage: needs reproduction', 'triage: unable to reproduce',
    'triage: unable to fix', 'triage: failed', 'triage: fix rejected',
]);
const bots = new Set(['github-actions[bot]', 'oxc-guard[bot]']);
const triageLabels = new Set([...retryLabels, 'triage: fix pending',
    'triage: not actionable', 'triage: skipped', 'triage: fix verified']);

export function route(event) {
    if (!event.issue || event.issue.pull_request) return 'skip';
    if (event.action === 'closed') return event.issue.state === 'closed' ? 'cleanup' : 'skip';
    if (event.issue.state === 'closed') return 'skip';
    if (event.action === 'opened' || event.action === 'reopened') return 'triage';
    if (event.action !== 'created' || bots.has(event.comment?.user?.login)) return 'skip';
    const labels = event.issue.labels.map(label => typeof label === 'string' ? label : label.name);
    const current = labels.find(label => triageLabels.has(label));
    if (current === 'triage: fix pending') return 'verify-fix';
    return retryLabels.has(current) ? 'retriage' : 'skip';
}

export async function preflight({ event, repo, token, fetchImpl = fetch }) {
    const eventRoute = route(event);
    if (eventRoute === 'skip') return { route: 'skip' };
    const response = await fetchImpl(`https://api.github.com/repos/${repo}/issues/${event.issue.number}`, {
        headers: { Authorization: `Bearer ${token}`, Accept: 'application/vnd.github+json' },
    });
    if (!response.ok) throw new Error(`Cannot refresh triage issue (HTTP ${response.status})`);
    const current = { ...event, issue: await response.json() };
    // Suppress queued events that became terminal. Keep the original event route
    // for setup because the unchanged upstream action consumes that same payload.
    return { route: route(current) === 'skip' ? 'skip' : eventRoute };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
    const result = await preflight({
        event: JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8')),
        repo: process.env.GITHUB_REPOSITORY,
        token: process.env.GH_TOKEN,
    });
    appendFileSync(process.env.GITHUB_OUTPUT, `route=${result.route}\n`);
    console.log(`Triage preflight: ${result.route}`);
}
