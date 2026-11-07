#!/usr/bin/env node
// Measures type-aware lint plus a type check, as one Oxlint run or as separate commands, on the projects setup.sh
// prepares.
//   sudo node measure.mjs run <rule set>     every setup of every project into ~/lint-typecheck; needs root for cgroups
//   node measure.mjs collect [results.json]  folds ~/lint-typecheck/results-<set>.jsonl and the run logs into results.json
// Env for run: ROUNDS (5), ESLINT_ROUNDS (ROUNDS), ONLY (substring of "<project> <setup>"), WARMUP=0 skips the warm-up
// runs. LINT_TYPECHECK_WORK moves the work directory (~/lint-typecheck).
// Each setup's processes start together inside one fresh cgroup v2 group: memory.peak is their combined peak and
// cpu.stat usage_usec their combined CPU time. The setup order rotates every round and reverses on odd rounds.
import { execFileSync, spawn } from 'node:child_process';
import {
  appendFileSync,
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readdirSync,
  readFileSync,
  rmdirSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const HOME = '/home/ubuntu';
const TOOLS = `${HOME}/tools`;
const WORK = process.env.LINT_TYPECHECK_WORK ?? `${HOME}/lint-typecheck`;
const CGROUP = '/sys/fs/cgroup';

export const PROJECTS = [
  { name: 'vscode', repo: 'microsoft/vscode', ref: '1.145.2', tsconfig: 'src/tsconfig.json' },
  // Outline's lint script passes these folders.
  {
    name: 'outline',
    repo: 'outline/outline',
    ref: 'v1.12.0',
    tsconfig: 'tsconfig.json',
    lintDirs: ['app/', 'server/', 'shared/', 'plugins/'],
  },
  { name: 'vue', repo: 'vuejs/core', ref: 'v3.5.45', tsconfig: 'tsconfig.json' },
];

export const RULE_SETS = {
  // The type-aware rules in Outline's own .oxlintrc.json at v1.12.0.
  outline: [
    'await-thenable',
    'no-duplicate-type-constituents',
    'no-floating-promises',
    'no-meaningless-void-operator',
    'no-useless-default-assignment',
    'require-array-sort-compare',
    'restrict-template-expressions',
  ],
  // typescript-eslint's recommended-type-checked rules that need type information.
  recommended: [
    'await-thenable',
    'no-array-delete',
    'no-base-to-string',
    'no-duplicate-type-constituents',
    'no-floating-promises',
    'no-for-in-array',
    'no-implied-eval',
    'no-misused-promises',
    'no-redundant-type-constituents',
    'no-unnecessary-type-assertion',
    'no-unsafe-argument',
    'no-unsafe-assignment',
    'no-unsafe-call',
    'no-unsafe-enum-comparison',
    'no-unsafe-member-access',
    'no-unsafe-return',
    'no-unsafe-unary-minus',
    'only-throw-error',
    'prefer-promise-reject-errors',
    'require-await',
    'restrict-plus-operands',
    'restrict-template-expressions',
    'unbound-method',
  ],
  // oxc-project/tsgolint benchmarks/eslint.config.mjs.
  all: [
    'await-thenable',
    'consistent-return',
    'consistent-type-exports',
    'dot-notation',
    'no-array-delete',
    'no-base-to-string',
    'no-confusing-void-expression',
    'no-deprecated',
    'no-duplicate-type-constituents',
    'no-floating-promises',
    'no-for-in-array',
    'no-implied-eval',
    'no-meaningless-void-operator',
    'no-misused-promises',
    'no-misused-spread',
    'no-mixed-enums',
    'no-redundant-type-constituents',
    'no-unnecessary-boolean-literal-compare',
    'no-unnecessary-condition',
    'no-unnecessary-qualifier',
    'no-unnecessary-template-expression',
    'no-unnecessary-type-conversion',
    'no-unnecessary-type-arguments',
    'no-unnecessary-type-assertion',
    'no-unnecessary-type-parameters',
    'no-useless-default-assignment',
    'no-unsafe-argument',
    'no-unsafe-assignment',
    'no-unsafe-call',
    'no-unsafe-enum-comparison',
    'no-unsafe-member-access',
    'no-unsafe-return',
    'no-unsafe-type-assertion',
    'no-unsafe-unary-minus',
    'non-nullable-type-assertion-style',
    'only-throw-error',
    'prefer-find',
    'prefer-includes',
    'prefer-nullish-coalescing',
    'prefer-optional-chain',
    'prefer-promise-reject-errors',
    'prefer-readonly',
    'prefer-readonly-parameter-types',
    'prefer-reduce-type-parameter',
    'prefer-regexp-exec',
    'prefer-return-this-type',
    'prefer-string-starts-ends-with',
    'promise-function-async',
    'related-getter-setter-pairs',
    'require-array-sort-compare',
    'require-await',
    'restrict-plus-operands',
    'restrict-template-expressions',
    'return-await',
    'strict-boolean-expressions',
    'strict-void-return',
    'switch-exhaustiveness-check',
    'unbound-method',
    'use-unknown-in-catch-callback-variable',
  ],
};

function setups(project, ruleSet) {
  const check = ['-p', project.tsconfig, '--noEmit', '--incremental', 'false', '--pretty', 'false'];
  const oxlint = (tool) => [
    `${TOOLS}/${tool}/node_modules/.bin/oxlint`,
    '-c',
    `${WORK}/oxlintrc.${ruleSet}.json`,
    '--type-aware',
    '--format',
    'unix',
  ];
  const tsrs = { TSRS_FAST: '1', OXLINT_TSGOLINT_PATH: `${TOOLS}/tsrs/node_modules/.bin/tsrs-lint` };
  const tsgolint = { OXLINT_TSGOLINT_PATH: `${TOOLS}/tsgolint/node_modules/.bin/tsgolint` };
  const eslintRounds = process.env.ESLINT_ROUNDS ? Number(process.env.ESLINT_ROUNDS) : undefined;
  return {
    'tsrs one pass': { env: tsrs, cmds: [[...oxlint('tsrs'), '--type-check', '@FILES']] },
    'tsrs separate': {
      env: tsrs,
      cmds: [
        [...oxlint('tsrs'), '@FILES'],
        [`${TOOLS}/tsrs/node_modules/.bin/tsrs`, ...check],
      ],
    },
    'tsrs lint only': { env: tsrs, cmds: [[...oxlint('tsrs'), '@FILES']] },
    'tsrs check only': { env: tsrs, cmds: [[`${TOOLS}/tsrs/node_modules/.bin/tsrs`, ...check]] },
    'tsgolint one pass': { env: tsgolint, cmds: [[...oxlint('tsgolint'), '--type-check', '@FILES']] },
    'tsgolint separate': {
      env: tsgolint,
      cmds: [
        [...oxlint('tsgolint'), '@FILES'],
        [`${TOOLS}/tsgolint/node_modules/.bin/tsc`, ...check],
      ],
    },
    // ESLint takes minutes per run, so it skips the warm-up run.
    'eslint separate': {
      rounds: eslintRounds,
      warmup: false,
      env: { NODE_OPTIONS: '--max-old-space-size=32768', BENCH_TSCONFIG: `./${project.tsconfig}` },
      cmds: [
        [
          'node',
          `${TOOLS}/eslint/node_modules/eslint/bin/eslint.js`,
          '--no-inline-config',
          '--config',
          `${project.dir}/eslint.bench.${ruleSet}.mjs`,
          '@FILES',
        ],
        ['node', `${TOOLS}/eslint/node_modules/typescript/bin/tsc`, ...check],
      ],
    },
  };
}

// Source files of the tsconfig program, without declaration files, as each linter receives them.
function lintFiles(project) {
  const listed = execFileSync(`${TOOLS}/tsgolint/node_modules/.bin/tsc`, ['-p', project.tsconfig, '--listFilesOnly'], {
    cwd: project.dir,
    encoding: 'utf8',
    maxBuffer: 1 << 28,
  });
  return listed
    .split('\n')
    .filter(Boolean)
    .map((file) => path.relative(project.dir, file))
    .filter(
      (file) =>
        !file.startsWith('..') &&
        !file.includes('node_modules/') &&
        /\.[cm]?tsx?$/.test(file) &&
        !/\.d\.[cm]?ts$/.test(file),
    )
    .filter((file) => !project.lintDirs || project.lintDirs.some((dir) => file.startsWith(dir)));
}

function writeConfigs(ruleSet) {
  const rules = RULE_SETS[ruleSet];
  writeFileSync(
    `${WORK}/oxlintrc.${ruleSet}.json`,
    JSON.stringify(
      {
        plugins: ['typescript'],
        categories: { correctness: 'off' },
        rules: Object.fromEntries(rules.map((rule) => [`typescript/${rule}`, 'error'])),
      },
      null,
      2,
    ) + '\n',
  );
  // ESLint matches `files` relative to the config file, so each project gets a copy at its root.
  const eslint = `import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const tseslint = createRequire('${TOOLS}/eslint/package.json')('typescript-eslint');

export default [
  { linterOptions: { noInlineConfig: true, reportUnusedInlineConfigs: 'off', reportUnusedDisableDirectives: 'off' } },
  {
    files: ['**/*.ts', '**/*.tsx', '**/*.mts', '**/*.cts'],
    languageOptions: {
      parser: tseslint.parser,
      parserOptions: { project: process.env.BENCH_TSCONFIG, tsconfigRootDir: path.dirname(fileURLToPath(import.meta.url)) },
    },
    plugins: { '@typescript-eslint': tseslint.plugin },
    rules: {
${rules.map((rule) => `      '@typescript-eslint/${rule}': 'error',`).join('\n')}
    },
  },
];
`;
  for (const project of PROJECTS) writeFileSync(`${project.dir}/eslint.bench.${ruleSet}.mjs`, eslint);
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
let groups = 0;

async function runSetup(project, name, setup, files, logDir, tag) {
  const cgroup = `${CGROUP}/lint-typecheck-${process.pid}-${groups++}`;
  mkdirSync(cgroup);
  const start = process.hrtime.bigint();
  const elapsed = () => Number(process.hrtime.bigint() - start) / 1e6;
  const processes = setup.cmds.map(
    (argv, index) =>
      new Promise((resolve) => {
        const log = `${logDir}/${name.replaceAll(' ', '-')}.${index}.${tag}.log`;
        const out = openSync(log, 'w');
        const child = spawn(
          path.join(HERE, 'cgexec.sh'),
          [cgroup, ...argv.flatMap((arg) => (arg === '@FILES' ? files : [arg]))],
          {
            cwd: project.dir,
            env: { PATH: '/usr/local/bin:/usr/bin:/bin', HOME, USER: 'ubuntu', LANG: 'C.UTF-8', ...setup.env },
            stdio: ['ignore', out, out],
          },
        );
        child.on('exit', (code, signal) => {
          closeSync(out);
          resolve({ code, signal, wallMs: elapsed(), log });
        });
      }),
  );
  const results = await Promise.all(processes);
  const wallMs = elapsed();
  const peakBytes = Number(readFileSync(`${cgroup}/memory.peak`, 'utf8'));
  const cpu = Object.fromEntries(
    readFileSync(`${cgroup}/cpu.stat`, 'utf8')
      .trim()
      .split('\n')
      .map((line) => line.split(' '))
      .map(([key, value]) => [key, Number(value)]),
  );
  for (let attempt = 0; ; attempt++) {
    try {
      rmdirSync(cgroup);
      break;
    } catch (error) {
      if (attempt > 50) throw error;
      await sleep(100);
    }
  }
  // Linters and compilers exit 1 or 2 when they report problems.
  const failed = results.some((r) => r.signal || ![0, 1, 2].includes(r.code));
  return {
    project: project.name,
    arm: name,
    wallMs,
    cpuMs: cpu.usage_usec / 1000,
    userMs: cpu.user_usec / 1000,
    systemMs: cpu.system_usec / 1000,
    peakMiB: peakBytes / 2 ** 20,
    cmds: results,
    failed,
  };
}

async function run(ruleSet) {
  const outPath = `${WORK}/results-${ruleSet}.jsonl`;
  if (!RULE_SETS[ruleSet]) throw new Error(`unknown rule set ${ruleSet}; one of ${Object.keys(RULE_SETS).join(', ')}`);
  const rounds = Number(process.env.ROUNDS ?? 5);
  const only = process.env.ONLY;
  const subtree = readFileSync(`${CGROUP}/cgroup.subtree_control`, 'utf8');
  if (!subtree.includes('memory') || !subtree.includes('cpu'))
    writeFileSync(`${CGROUP}/cgroup.subtree_control`, '+memory +cpu');
  for (const project of PROJECTS) project.dir = `${HOME}/proj/${project.name}`;
  writeConfigs(ruleSet);
  for (const project of PROJECTS) {
    const jobs = Object.entries(setups(project, ruleSet)).filter(
      ([name]) => !only || `${project.name} ${name}`.includes(only),
    );
    if (!jobs.length) continue;
    const files = lintFiles(project);
    console.log(`${project.name}: ${files.length} files`);
    const logDir = `${WORK}/logs-${ruleSet}/${project.name}`;
    mkdirSync(logDir, { recursive: true });
    if (process.env.WARMUP !== '0') {
      for (const [name, setup] of jobs.filter(([, setup]) => setup.warmup !== false)) {
        const sample = await runSetup(project, name, setup, files, logDir, 'warm');
        console.log(
          `WARM ${project.name} ${name} wall=${(sample.wallMs / 1000).toFixed(2)}s${sample.failed ? ` FAIL ${JSON.stringify(sample.cmds)}` : ''}`,
        );
      }
    }
    for (let round = 0; round < rounds; round++) {
      const shift = round % jobs.length;
      const order = [...jobs.slice(shift), ...jobs.slice(0, shift)];
      if (round % 2 === 1) order.reverse();
      for (const [name, setup] of order) {
        if (setup.rounds !== undefined && round >= setup.rounds) continue;
        const sample = await runSetup(project, name, setup, files, logDir, `r${round}`);
        appendFileSync(
          outPath,
          JSON.stringify({ ...sample, round, files: files.length, at: new Date().toISOString() }) + '\n',
        );
        console.log(
          `${project.name} r${round} ${name} wall=${(sample.wallMs / 1000).toFixed(2)}s cpu=${(sample.cpuMs / 1000).toFixed(1)}s peak=${sample.peakMiB.toFixed(0)}MiB${sample.failed ? ` FAIL ${JSON.stringify(sample.cmds)}` : ''}`,
        );
      }
    }
  }
  console.log('RUN DONE');
}

// Findings in the first timed round's logs: Oxlint's unix format ends with "N problems", ESLint's stylish format with
// "✖ N problems (...)", and tsc with --pretty false prints one "error TS" line per error.
function findings(logDir) {
  const out = {};
  for (const file of readdirSync(logDir)
    .filter((f) => f.endsWith('.r0.log'))
    .sort()) {
    const [setup, index] = file.split('.');
    const text = readFileSync(`${logDir}/${file}`, 'utf8');
    const oxlint = text
      .trimEnd()
      .split('\n')
      .at(-1)
      ?.match(/^(\d+) problems?$/);
    const eslint = text.match(/✖ (\d+) problems? \((\d+) errors?, (\d+) warnings?\)/);
    const count = oxlint
      ? {
          tool: 'oxlint',
          findings: Number(oxlint[1]),
          typeErrors: (text.match(/\[Error\/typescript\(TS\d+\)\]/g) ?? []).length,
        }
      : eslint
        ? { tool: 'eslint', findings: Number(eslint[1]) }
        : { typeErrors: (text.match(/error TS\d+/g) ?? []).length };
    (out[setup.replaceAll('-', ' ')] ??= [])[Number(index)] = count;
  }
  return out;
}

function collect(outPath = path.join(HERE, 'results.json')) {
  const dir = WORK;
  const results = { measured: null, machine: null, versions: null, projects: [], rules: {}, samples: {}, findings: {} };
  const previous = existsSync(outPath) ? JSON.parse(readFileSync(outPath, 'utf8')) : {};
  for (const key of ['measured', 'machine', 'versions']) results[key] = previous[key] ?? null;
  const fileCounts = {};
  for (const ruleSet of Object.keys(RULE_SETS)) {
    const jsonl = `${dir}/results-${ruleSet}.jsonl`;
    if (!existsSync(jsonl)) continue;
    results.rules[ruleSet] = RULE_SETS[ruleSet];
    results.samples[ruleSet] = {};
    for (const line of readFileSync(jsonl, 'utf8').trim().split('\n')) {
      const row = JSON.parse(line);
      if (row.failed) throw new Error(`failed sample in ${jsonl}: ${line}`);
      fileCounts[row.project] = row.files;
      ((results.samples[ruleSet][row.project] ??= {})[row.arm] ??= []).push({
        round: row.round,
        wallMs: Math.round(row.wallMs),
        cpuMs: Math.round(row.cpuMs),
        peakMiB: Math.round(row.peakMiB),
        processWallMs: row.cmds.map((c) => Math.round(c.wallMs)),
      });
    }
    results.findings[ruleSet] = {};
    for (const project of PROJECTS) {
      const logDir = `${dir}/logs-${ruleSet}/${project.name}`;
      if (existsSync(logDir)) results.findings[ruleSet][project.name] = findings(logDir);
    }
  }
  results.projects = PROJECTS.map(({ name, repo, ref, tsconfig, lintDirs }) => ({
    name,
    repo,
    ref,
    tsconfig,
    lintDirs,
    lintFiles: fileCounts[name],
  }));
  writeFileSync(outPath, JSON.stringify(results, null, 2) + '\n');
  console.log(`wrote ${outPath}`);
}

const [command, ...rest] = process.argv.slice(2);
if (command === 'run') await run(...rest);
else if (command === 'collect') collect(...rest);
else {
  console.error('usage: sudo node measure.mjs run <rule set> | node measure.mjs collect [results.json]');
  process.exit(2);
}
