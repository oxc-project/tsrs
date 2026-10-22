import { appendFileSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const retryLabels = new Set([
    'triage: needs triage', 'triage: needs reproduction', 'triage: unable to reproduce',
    'triage: unable to fix', 'triage: failed', 'triage: fix rejected',
]);
const bots = new Set(['github-actions[bot]', 'oxc-guard[bot]']);

export function route(event) {
    if (!event.issue || event.issue.pull_request) return 'skip';
    if (event.action === 'closed') return event.issue.state === 'closed' ? 'cleanup' : 'skip';
    if (event.issue.state === 'closed') return 'skip';
    if (event.action === 'opened' || event.action === 'reopened') return 'triage';
    if (event.action !== 'created' || bots.has(event.comment?.user?.login)) return 'skip';
    const labels = event.issue.labels.map(label => typeof label === 'string' ? label : label.name);
    if (labels.includes('triage: fix pending')) return 'verify-fix';
    return labels.some(label => retryLabels.has(label)) ? 'retriage' : 'skip';
}

export async function preflight({ event, repo, token, fetchImpl = fetch }) {
    if (!event.issue || event.issue.pull_request || bots.has(event.comment?.user?.login)) {
        return { event, route: 'skip' };
    }
    const response = await fetchImpl(`https://api.github.com/repos/${repo}/issues/${event.issue.number}`, {
        headers: { Authorization: `Bearer ${token}`, Accept: 'application/vnd.github+json' },
    });
    if (!response.ok) throw new Error(`Cannot refresh triage issue (HTTP ${response.status})`);
    const current = { ...event, issue: await response.json() };
    return { event: current, route: route(current) };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
    const result = await preflight({
        event: JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8')),
        repo: process.env.GITHUB_REPOSITORY,
        token: process.env.GH_TOKEN,
    });
    const eventPath = resolve(process.env.RUNNER_TEMP, 'tsrs-triage-event.json');
    writeFileSync(eventPath, JSON.stringify(result.event));
    appendFileSync(process.env.GITHUB_OUTPUT, `route=${result.route}\nevent-path=${eventPath}\n`);
    console.log(`Triage preflight: ${result.route}`);
}
