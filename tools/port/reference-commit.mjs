#!/usr/bin/env node
// PR CI tests a rolling port against its latest source, while main and other
// PRs continue to test the published TypeScript package pin.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { latestLedgerCommit } from './queue.mjs';

if (process.env.GITHUB_HEAD_REF?.startsWith('port/typescript-sync/')) {
  const sha = latestLedgerCommit(readFileSync('notes/typescript-continuous-ports.md', 'utf8'));
  if (!sha) throw new Error('Rolling port PR has no ledger entry');
  console.log(sha);
} else {
  process.stdout.write(execFileSync('node', ['npm/build.mjs', '--print-typescript-commit'], {
    encoding: 'utf8',
  }));
}
