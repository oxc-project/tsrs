// Pure queue logic; the workflow supplies the upstream graph and local port trailers.
export const MAINLINE_ANCHOR = '5237fd46890956e4bb8177a1a05dcd69e4166560';

export function portedCommits(log, ledger = '') {
  const trailers = [...log.matchAll(/^Upstream-Commit:\s*([0-9a-f]{40})\s*$/gim)];
  const entries = [...ledger.matchAll(/^## ([0-9a-f]{40})\s+—\s+/gim)];
  return new Set([...trailers, ...entries].map((match) => match[1].toLowerCase()));
}

export function latestLedgerCommit(ledger) {
  return [...ledger.matchAll(/^## ([0-9a-f]{40})\s+—\s+/gim)].at(-1)?.[1]?.toLowerCase();
}

export function pendingCommits(revList, ported) {
  return revList
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .map((sha) => sha.toLowerCase())
    .filter((sha) => /^[0-9a-f]{40}$/.test(sha) && !ported.has(sha));
}

export function checkStates(checks) {
  if (!checks.length) return 'pending';
  if (checks.some((check) => check.bucket === 'fail' || check.bucket === 'cancel')) return 'failed';
  if (checks.every((check) => check.bucket === 'pass' || check.bucket === 'skipping')) return 'passed';
  if (checks.some((check) => check.bucket)) return 'pending';
  const passed = new Set(['SUCCESS', 'SKIPPED', 'NEUTRAL']);
  const failed = new Set(['FAILURE', 'ERROR', 'CANCELLED', 'TIMED_OUT', 'ACTION_REQUIRED']);
  if (checks.some((check) => failed.has(check.state))) return 'failed';
  if (checks.every((check) => passed.has(check.state))) return 'passed';
  return 'pending';
}
