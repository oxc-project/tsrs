#!/usr/bin/env node
// One upstream commit per run. GitHub Actions serializes scheduled and manual
// runs, and each run waits for the current PR's checks before selecting a port.
import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { checkStates, latestLedgerCommit, MAINLINE_ANCHOR, pendingCommits, portedCommits } from './queue.mjs';

const root = process.cwd();
const work = join(root, 'target', 'port-agent');
const metadataPath = join(work, 'metadata.json');
const branchPrefix = 'port/typescript-sync/';
const repository = process.env.GITHUB_REPOSITORY || 'oxc-project/tsrs';

function command(program, args, options = {}) {
  return execFileSync(program, args, {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    maxBuffer: 64 * 1024 * 1024,
    ...options,
  }).trim();
}

function run(program, args, options = {}) {
  const result = spawnSync(program, args, {
    cwd: root,
    stdio: 'inherit',
    ...options,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${program} failed with status ${result.status}`);
}

function git(...args) {
  return command('git', args);
}

function gh(...args) {
  return command('gh', args);
}

function output(values) {
  if (!process.env.GITHUB_OUTPUT) return;
  const fs = Object.entries(values)
    .map(([key, value]) => `${key}=${String(value).replaceAll('\n', ' ')}`)
    .join('\n');
  const previous = existsSync(process.env.GITHUB_OUTPUT)
    ? readFileSync(process.env.GITHUB_OUTPUT, 'utf8')
    : '';
  writeFileSync(process.env.GITHUB_OUTPUT, `${previous}${fs}\n`);
}

function save(metadata) {
  mkdirSync(work, { recursive: true });
  writeFileSync(metadataPath, `${JSON.stringify(metadata, null, 2)}\n`);
}

function load() {
  return JSON.parse(readFileSync(metadataPath, 'utf8'));
}

function openBlocker(sha) {
  const title = `Port blocked: microsoft/TypeScript@${sha}`;
  return JSON.parse(gh('issue', 'list', '--state', 'open', '--search', sha,
    '--limit', '100', '--json', 'number,title'))
    .find((issue) => issue.title === title);
}

function findPortPr() {
  const prs = JSON.parse(gh('pr', 'list', '--state', 'open', '--limit', '1000', '--json',
    'number,headRefName,baseRefName,url,isCrossRepository,mergeable'))
    .filter((pr) => pr.baseRefName === 'main' && !pr.isCrossRepository &&
      pr.headRefName.startsWith(branchPrefix));
  if (prs.length > 1) throw new Error(`Multiple open TypeScript port PRs: ${prs.map((pr) => pr.url).join(', ')}`);
  return prs[0];
}

function prCheckState(pr) {
  const result = spawnSync('gh', ['pr', 'checks', String(pr.number), '--json', 'name,bucket'], {
    cwd: root,
    encoding: 'utf8',
  });
  if (result.error) throw result.error;
  if (!result.stdout?.trim()) {
    if (result.status !== 0) throw new Error(`Unable to read checks for ${pr.url}: ${result.stderr}`);
    return 'pending';
  }
  return checkStates(JSON.parse(result.stdout));
}

function checkoutReference() {
  const ref = join(root, 'ts-ref');
  if (!existsSync(join(ref, '.git'))) {
    run('git', ['clone', '--filter=blob:none', '--no-checkout', '--single-branch',
      '--branch', 'main', 'https://github.com/microsoft/TypeScript.git', 'ts-ref']);
  } else {
    run('git', ['-C', ref, 'fetch', '--filter=blob:none', 'origin', 'main']);
  }
  const head = command('git', ['-C', ref, 'rev-parse', 'origin/main']);
  run('git', ['-C', ref, 'merge-base', '--is-ancestor', MAINLINE_ANCHOR, head]);
  return { ref, head };
}

function prepare() {
  if (repository !== 'oxc-project/tsrs') throw new Error(`Unexpected repository: ${repository}`);
  const pr = findPortPr();
  if (pr) {
    if (!/^port\/typescript-sync\/[0-9]+$/.test(pr.headRefName)) {
      throw new Error(`Unexpected port branch: ${pr.headRefName}`);
    }
    run('git', ['fetch', 'origin', `refs/heads/${pr.headRefName}`]);
    run('git', ['checkout', '-B', pr.headRefName, 'FETCH_HEAD']);
    const check = prCheckState(pr);
    if (check === 'pending') {
      output({ status: 'waiting' });
      console.log(`Waiting for checks on ${pr.url}`);
      return;
    }
    if (check === 'failed') {
      const ledger = readFileSync(join(root, 'notes', 'typescript-continuous-ports.md'), 'utf8');
      const last = latestLedgerCommit(ledger);
      if (!last) throw new Error(`Failed PR checks, but ${pr.url} has no upstream trailer`);
      if (openBlocker(last)) {
        output({ status: 'waiting' });
        console.log(`PR checks remain failed; blocker for ${last} is already open`);
        return;
      }
      save({ sha: last, branch: pr.headRefName, reason: `Checks failed on ${pr.url}` });
      output({ status: 'blocked', sha: last });
      return;
    }
    if (pr.mergeable === 'CONFLICTING') {
      const last = latestLedgerCommit(readFileSync(join(root, 'notes', 'typescript-continuous-ports.md'), 'utf8'));
      if (!last) throw new Error(`Conflicting PR ${pr.url} has no port ledger entry`);
      if (openBlocker(last)) {
        output({ status: 'waiting' });
        return;
      }
      save({ sha: last, branch: pr.headRefName, reason: `Merge conflict on ${pr.url}` });
      output({ status: 'blocked', sha: last });
      return;
    }
  }

  const { ref, head } = checkoutReference();
  const revList = command('git', ['-C', ref, 'rev-list', '--reverse', '--topo-order',
    `${MAINLINE_ANCHOR}..${head}`]);
  const history = `${git('log', '--format=%B')}\n${git('log', '--format=%B', 'origin/main')}`;
  const ledgerPath = join(root, 'notes', 'typescript-continuous-ports.md');
  const branchLedger = existsSync(ledgerPath) ? readFileSync(ledgerPath, 'utf8') : '';
  const mainLedger = command('git', ['show', 'origin/main:notes/typescript-continuous-ports.md']);
  const ledger = `${branchLedger}\n${mainLedger}`;
  const pending = pendingCommits(revList, portedCommits(history, ledger));
  console.log(`${pending.length} upstream commits are not ported; TypeScript main is ${head}`);
  const sha = pending[0];
  if (!sha) {
    output({ status: 'empty' });
    return;
  }
  const requestedRetry = process.env.RETRY_SHA?.trim().toLowerCase();
  const previousPort = latestLedgerCommit(branchLedger);
  if (requestedRetry && requestedRetry !== sha && !(pr && requestedRetry === previousPort)) {
    throw new Error(`Requested retry ${requestedRetry} is not the next pending commit ${sha}`);
  }
  const blocker = openBlocker(sha);
  if (blocker && requestedRetry !== sha) {
    console.log(`Blocked by issue #${blocker.number}; dispatch manually with retry_sha=${sha} after intervention`);
    output({ status: 'waiting' });
    return;
  }

  const branch = pr?.headRefName || `${branchPrefix}${process.env.GITHUB_RUN_ID}`;
  if (!pr) run('git', ['checkout', '-b', branch]);
  const base = git('rev-parse', 'HEAD');
  const subject = command('git', ['-C', ref, 'show', '-s', '--format=%s', sha]);
  run('git', ['-C', ref, 'sparse-checkout', 'set', 'tsc', 'packages/typescript/src',
    'packages/typescript/lib', 'packages/typescript/vendor', 'tools/scripts/tsc']);
  run('git', ['-C', ref, 'checkout', '--detach', sha]);
  const metadata = {
    sha, subject, branch, base, pr: pr?.number || null, previousPort,
    head, run: process.env.GITHUB_RUN_ID,
  };
  save(metadata);
  output({ status: 'ready', sha, branch });
  console.log(`Selected ${sha}: ${subject}`);
}

function configureOpenCode() {
  const account = process.env.CLOUDFLARE_ACCOUNT_ID;
  const gateway = process.env.CLOUDFLARE_GATEWAY_ID;
  const token = process.env.CLOUDFLARE_AI_GATEWAY_TOKEN;
  if (!account || !gateway || !token) {
    throw new Error('CLOUDFLARE_ACCOUNT_ID, CLOUDFLARE_GATEWAY_ID, and CLOUDFLARE_AI_GATEWAY_TOKEN are required');
  }
  if (!/^[a-zA-Z0-9_-]+$/.test(account) || !/^[a-zA-Z0-9_-]+$/.test(gateway)) {
    throw new Error('Invalid Cloudflare account or gateway ID');
  }
  const configDirectory = join(homedir(), '.config', 'opencode');
  mkdirSync(configDirectory, { recursive: true });
  const config = {
    $schema: 'https://opencode.ai/config.json',
    model: 'openai/gpt-6.1-sol',
    update: 'disable',
    providers: {
      openai: {
        package: '@opencode/ai/providers/openai/responses',
        settings: {
          baseURL: `https://gateway.ai.cloudflare.com/v1/${account}/${gateway}/openai`,
          apiKey: '',
        },
        headers: { 'cf-aig-authorization': 'Bearer {env:CLOUDFLARE_AI_GATEWAY_TOKEN}' },
        models: {
          'gpt-6.1-sol': {
            modelID: 'gpt-6.1-sol',
            name: 'GPT-6.1 Sol',
            capabilities: {
              tools: true,
              input: ['text', 'image'],
              output: ['text'],
              reasoning: { supported: true, effort: ['low', 'medium', 'high', 'xhigh', 'max'] },
            },
            limit: { context: 1_050_000, output: 128_000 },
          },
        },
      },
    },
  };
  writeFileSync(join(configDirectory, 'opencode.json'), `${JSON.stringify(config, null, 2)}\n`, { mode: 0o600 });
  if (!command('opencode', ['models']).includes('openai/gpt-6.1-sol')) {
    throw new Error('OpenCode did not register openai/gpt-6.1-sol from the gateway configuration');
  }
}

function verify(metadata) {
  if (git('rev-parse', 'HEAD') !== metadata.base) throw new Error('Agent changed Git HEAD');
  if (!existsSync(join(work, 'report.md'))) throw new Error('Agent did not write target/port-agent/report.md');
  const report = readFileSync(join(work, 'report.md'), 'utf8').trim();
  if (!report || report.length > 16_000) throw new Error('Agent report is empty or too long');
  const changed = git('status', '--porcelain');
  if (/^(?:.{2} )?(?:\.github\/workflows\/|tools\/port\/)/m.test(changed)) {
    throw new Error('Agent modified the port automation');
  }
  run('git', ['diff', '--check']);
  const validationEnv = { ...process.env };
  delete validationEnv.CLOUDFLARE_AI_GATEWAY_TOKEN;
  run('cargo', ['fmt', '--all', '--', '--check'], { env: validationEnv });
  run('cargo', ['build', '--profile', 'dev', '--locked', '-p', 'tsrs_cli', '-p', 'tsrs_testrunner'], { env: validationEnv });
  run('cargo', ['test', '--locked'], { env: validationEnv });
  run('cargo', ['clippy', '--locked', '--all-targets', '--config', 'build.warnings="deny"'], { env: validationEnv });
  run('python3', ['tools/lint/source.py'], { env: validationEnv });
  rmSync(join(work, 'test-results'), { recursive: true, force: true });
  run('.github/scripts/conformance-gate.sh', [], {
    env: {
      ...validationEnv,
      TSRS_TEST: 'target/debug/tsrs-test',
      TSRS_TEST_RESULTS: join(work, 'test-results'),
    },
  });
}

function agent() {
  const metadata = load();
  try {
    configureOpenCode();
  } catch (error) {
    writeFileSync(join(work, 'failure.txt'), error.message);
    throw error;
  }
  const prompt = readFileSync(join(root, 'tools/port/agent-prompt.md'), 'utf8')
    .replaceAll('{SHA}', metadata.sha)
    .replaceAll('{SUBJECT}', metadata.subject);
  let lastError;
  for (let attempt = 1; attempt <= 2; attempt++) {
    console.log(`Port attempt ${attempt}/2 for ${metadata.sha}`);
    run('git', ['reset', '--hard', metadata.base]);
    run('git', ['clean', '-fd']);
    mkdirSync(work, { recursive: true });
    rmSync(join(work, 'report.md'), { force: true });
    try {
      run('opencode', ['run', '--standalone', '--model', 'openai/gpt-6.1-sol', prompt], {
        timeout: 130 * 60 * 1000,
      });
      verify(metadata);
      writeFileSync(join(work, 'attempts.txt'), String(attempt));
      return;
    } catch (error) {
      lastError = error;
      console.error(`Attempt ${attempt} failed: ${error.message}`);
    }
  }
  writeFileSync(join(work, 'failure.txt'), lastError?.message || 'Unknown agent failure');
  throw lastError;
}

function configureBot() {
  const id = gh('api', 'users/oxc-guard[bot]', '--jq', '.id');
  run('git', ['config', 'user.name', 'oxc-guard[bot]']);
  run('git', ['config', 'user.email', `${id}+oxc-guard[bot]@users.noreply.github.com`]);
  run('gh', ['auth', 'setup-git']);
}

function publish() {
  const metadata = load();
  if (git('rev-parse', 'HEAD') !== metadata.base) throw new Error('Branch changed before publish');
  if (metadata.pr) {
    const pr = JSON.parse(gh('pr', 'view', String(metadata.pr), '--json', 'state,headRefName'));
    if (pr.state !== 'OPEN' || pr.headRefName !== metadata.branch) {
      throw new Error(`Port PR #${metadata.pr} changed or closed while the agent ran`);
    }
  }
  const report = readFileSync(join(work, 'report.md'), 'utf8').trim();
  const ledger = join(root, 'notes', 'typescript-continuous-ports.md');
  const existing = existsSync(ledger)
    ? readFileSync(ledger, 'utf8')
    : '# Continuous TypeScript ports\n\nEach entry corresponds to one upstream commit and one tsrs commit.\n';
  if (portedCommits('', existing).has(metadata.sha)) throw new Error(`Ledger already contains ${metadata.sha}`);
  writeFileSync(ledger, `${existing.trimEnd()}\n\n## ${metadata.sha} — ${metadata.subject}\n\n${report}\n`);
  run('git', ['add', '--all', '--', '.', ':!ts-ref', ':!target']);
  if (!git('diff', '--cached', '--name-only').split('\n').includes('notes/typescript-continuous-ports.md')) {
    throw new Error('Port evidence was not staged');
  }
  run('git', ['diff', '--cached', '--check']);
  configureBot();
  run('git', ['commit', '-m', `chore(port): sync TypeScript ${metadata.sha.slice(0, 12)}`,
    '-m', `Upstream-Commit: ${metadata.sha}`]);
  run('git', ['push', 'origin', `HEAD:refs/heads/${metadata.branch}`]);
  if (!metadata.pr) {
    const body = `Ports microsoft/TypeScript@${metadata.sha}. More commits will be added in upstream order.`;
    gh('pr', 'create', '--draft', '--base', 'main', '--head', metadata.branch,
      '--title', `chore(port): sync TypeScript upstream`, '--body', body);
  }
  const blocker = openBlocker(metadata.sha);
  if (blocker) run('gh', ['issue', 'close', String(blocker.number)]);
  if (metadata.previousPort) {
    const previousBlocker = openBlocker(metadata.previousPort);
    if (previousBlocker) run('gh', ['issue', 'close', String(previousBlocker.number)]);
  }
  console.log(`Published ${metadata.sha} to ${metadata.branch}`);
}

function block() {
  const metadata = load();
  const sha = metadata.sha;
  const existing = openBlocker(sha);
  const reason = metadata.reason || process.env.PORT_BLOCK_REASON || (existsSync(join(work, 'failure.txt'))
    ? readFileSync(join(work, 'failure.txt'), 'utf8').trim()
    : 'Agent, validation, or publication failed.');
  const url = `https://github.com/${repository}/actions/runs/${process.env.GITHUB_RUN_ID}`;
  const body = `The automatic port of microsoft/TypeScript@${sha} stopped after repeated port attempts, a failed PR check, or an automation error.\n\nReason: ${reason}\n\nRun: ${url}\n\nAfter fixing the problem, dispatch **Port TypeScript commits** with retry_sha=\`${sha}\`, or close this issue to let the next scheduled run retry.`;
  if (existing) {
    console.log(`Existing blocker: #${existing.number}`);
  } else {
    gh('issue', 'create', '--title', `Port blocked: microsoft/TypeScript@${sha}`, '--body', body);
  }
  throw new Error(`Port blocked at ${sha}: ${reason}`);
}

try {
  switch (process.argv[2]) {
    case 'prepare': prepare(); break;
    case 'agent': agent(); break;
    case 'publish': publish(); break;
    case 'block': block(); break;
    default: throw new Error('Usage: continuous.mjs prepare|agent|publish|block');
  }
} catch (error) {
  if (process.argv[2] === 'publish') {
    writeFileSync(join(work, 'failure.txt'), error.message);
  }
  console.error(error.message);
  process.exitCode = 1;
}
