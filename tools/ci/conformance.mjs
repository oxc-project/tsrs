// Conformance gate: every baseline in Go history and in the default CLI mode, then a scheduling sweep over
// testdata/checker-determinism.txt with work stealing and seeded assignments.
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({
    options: {
        mode: { type: 'string', default: 'all' },
        list: { type: 'string' },
        filter: { type: 'string' },
        jobs: { type: 'string' },
        'default-reps': { type: 'string', default: '10' },
        seeds: { type: 'string', default: '8' },
    },
});
if (!['all', 'go', 'default', 'determinism'].includes(values.mode)) {
    throw new Error('--mode must be all, go, default, or determinism');
}
function positive(name) {
    const value = Number(values[name]);
    if (!Number.isSafeInteger(value) || value < 1) throw new Error(`--${name} must be a positive integer`);
    return value;
}
const repetitions = positive('default-reps');
const seeds = positive('seeds');
const binary = resolve(process.env.TSRS_TEST || 'target/debug/tsrs-test');
const results = resolve(process.env.TSRS_TEST_RESULTS || 'target/test-results');
const args = ['run', '--suite', 'all', '--baselines', 'all', '--timeout', process.env.TEST_TIMEOUT || '120'];
for (const name of ['list', 'filter', 'jobs']) {
    if (values[name]) args.push(`--${name}`, values[name]);
}

let failed = false;
function run(label, directory, { go = false, checkers, assignment = '', focused = false } = {}) {
    const extra = [];
    if (checkers !== undefined) extra.push('--checkers', String(checkers));
    if (focused && !values.list && !values.filter) extra.push('--list', 'testdata/checker-determinism.txt');
    console.log(`conformance: ${label}`);
    const result = spawnSync(binary, [...args, ...extra], {
        stdio: 'inherit',
        env: {
            ...process.env,
            TS_TEST_PROGRAM_SINGLE_THREADED: String(go),
            TSRS_HISTORY: go ? 'go' : 'canonical',
            TSRS_CHECKER_ASSIGNMENT: go ? 'go' : assignment,
            TSRS_TEST_RESULTS: directory,
        },
    });
    if (result.error) throw result.error;
    if (result.status !== 0) {
        console.error(`conformance: ${label} failed (${result.signal || result.status})`);
        failed = true;
    }
}

if (values.mode === 'all' || values.mode === 'go') run('Go baseline history', results, { go: true });
if (values.mode === 'all' || values.mode === 'default') run('default CLI mode', `${results}-default`);
if (values.mode === 'all' || values.mode === 'determinism') {
    // Every run compares every requested baseline with upstream, so identical crashes never become a reference.
    for (const checkers of [4, 8]) {
        for (let rep = 1; rep <= repetitions; rep++) {
            const label = `default-${checkers}-${rep}`;
            run(label, `${results}-determinism/${label}`, { checkers, focused: true });
        }
        for (let seed = 1; seed <= seeds; seed++) {
            const label = `random-${checkers}-${seed}`;
            run(label, `${results}-determinism/${label}`, { checkers, assignment: `random:${seed}`, focused: true });
        }
    }
}
process.exitCode = failed ? 1 : 0;
