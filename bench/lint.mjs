#!/usr/bin/env node
// Compares main and PR tsrs-lint on pinned public projects: alternating timed pairs and a diagnostics diff.
import fs from "node:fs";
import path from "node:path";
import { ROOT, args, json, writeJSON, run, median, required, requireValue, main, isMain } from "../tools/lib/cli.mjs";
import { hyperfine } from "./runner.mjs";
import { cleanEnvironment, shellJoin } from "./lib.mjs";
import { canonicalJSON, readDiagnostics } from "./tsgolint.mjs";

const BENCH = path.join(ROOT, "bench");
// Pinned, like the projects' commits, so main and the PR lint the same files with the same rules.
export const LINT_PROJECTS = ["webpack", "drizzle-orm", "xstate-main"];
// Every rule tsrs-lint implements (registry.rs), with default options.
const RULES = [
    "await-thenable", "consistent-return", "consistent-type-exports", "dot-notation", "no-array-delete",
    "no-base-to-string", "no-confusing-void-expression", "no-deprecated", "no-duplicate-type-constituents",
    "no-floating-promises", "no-for-in-array", "no-generated-empty-object-type", "no-implied-eval",
    "no-meaningless-void-operator", "no-misused-promises", "no-misused-spread", "no-mixed-enums",
    "no-redundant-type-constituents", "no-unnecessary-boolean-literal-compare", "no-unnecessary-condition",
    "no-unnecessary-qualifier", "no-unnecessary-template-expression", "no-unnecessary-type-conversion",
    "no-unnecessary-type-arguments", "no-unnecessary-type-parameters", "no-unnecessary-type-assertion",
    "no-useless-default-assignment", "no-unsafe-argument", "no-unsafe-assignment", "no-unsafe-call",
    "no-unsafe-enum-assignment", "no-unsafe-enum-comparison", "no-unsafe-member-access", "no-unsafe-return",
    "no-unsafe-type-assertion", "no-unsafe-unary-minus", "non-nullable-type-assertion-style", "only-throw-error",
    "prefer-find", "prefer-includes", "prefer-optional-chain", "prefer-nullish-coalescing",
    "prefer-promise-reject-errors", "prefer-readonly-parameter-types", "prefer-regexp-exec", "prefer-readonly",
    "prefer-reduce-type-parameter", "prefer-return-this-type", "prefer-string-starts-ends-with",
    "promise-function-async", "related-getter-setter-pairs", "require-array-sort-compare", "require-await",
    "restrict-plus-operands", "restrict-template-expressions", "return-await", "strict-boolean-expressions",
    "strict-void-return", "switch-exhaustiveness-check", "unbound-method", "use-unknown-in-catch-callback-variable",
];
// Both checker presets: TSRS_FAST unset (TypeScript's defaults) and TSRS_FAST=1.
const MODES = { default: null, fast: "1" };
const METRICS = "time_wall_clock:s,time_cpu:s,memory_peak_resident:B";
const SHOWN_DIFFERENCES = 5;

// The source files the project's tsconfig checks, as oxlint would pass them: no declaration files or dependencies.
// Real paths, because workspace packages are listed through their node_modules links.
function lintFiles(tsrs, cwd, config, root) {
    const listed = run([tsrs, "-p", config, "--listFilesOnly"], { cwd })
        .stdout.split("\n")
        .filter((file) => path.isAbsolute(file) && /\.[cm]?[jt]sx?$/.test(file) && !/\.d\.[cm]?ts$/.test(file))
        .map((file) => fs.realpathSync(file));
    return [...new Set(listed)]
        .filter((file) => file.startsWith(root + path.sep) && !file.includes(`${path.sep}node_modules${path.sep}`))
        .sort();
}

function lintOnce(binary, request, cwd, env, output) {
    const stdin = fs.openSync(request, "r"),
        stdout = fs.openSync(output, "w");
    try {
        const p = run([binary, "headless"], { cwd, env, check: false, stdio: [stdin, stdout, "pipe"] });
        requireValue(p.code === 0, `${binary} exited ${p.code}: ${p.stderr.trim()}`);
    } finally {
        fs.closeSync(stdin);
        fs.closeSync(stdout);
    }
    return readDiagnostics(output);
}

function compareDiagnostics(onMain, onPr, root) {
    const label = (d) => {
        const file = d.file_path ? path.relative(root, d.file_path) : "<no file>";
        return `${d.rule ?? "<compiler>"} ${file}:${d.range?.pos ?? "-"} ${d.message?.id ?? ""}`.trim();
    };
    const keyed = (list) => list.map((d) => [canonicalJSON({ ...d, file_path: d.file_path && path.relative(root, d.file_path) }), d]);
    const only = (a, b) => {
        const left = new Map();
        for (const [key] of b) left.set(key, (left.get(key) ?? 0) + 1);
        return a.filter(([key]) => {
            const n = left.get(key) ?? 0;
            left.set(key, n - 1);
            return n <= 0;
        });
    };
    const [m, p] = [keyed(onMain), keyed(onPr)],
        onlyMain = only(m, p),
        onlyPr = only(p, m);
    return {
        main: onMain.length,
        pr: onPr.length,
        only_main: onlyMain.length,
        only_pr: onlyPr.length,
        first_only_main: onlyMain.slice(0, SHOWN_DIFFERENCES).map(([, d]) => label(d)),
        first_only_pr: onlyPr.slice(0, SHOWN_DIFFERENCES).map(([, d]) => label(d)),
    };
}

function sample(hf, binary, request, cwd, env, output) {
    run(
        [hf, "--shell=none", "--runs", "1", "--warmup", "0", "--metrics", METRICS, "--style", "none",
            "--input", request, "--export-json", output, "--command-name", "tsrs-lint", shellJoin([binary, "headless"])],
        { cwd, env },
    );
    const data = json(output);
    requireValue(data.schema_version === 2, `${output}: unsupported Hyperfine schema`);
    const m = data.results?.[0]?.measurements?.[0] ?? {};
    requireValue(m.exit_code === 0, `${output}: tsrs-lint failed`);
    const value = (name, unit) => {
        requireValue(m[name]?.unit === unit && m[name].value > 0, `${output}: invalid ${name}`);
        return m[name].value;
    };
    return {
        wall_s: value("time_wall_clock", "second"),
        cpu_s: value("time_cpu", "second"),
        peak_rss_bytes: value("memory_peak_resident", "byte"),
    };
}

async function measureProject(name, a, hf, out) {
    const cfg = json(path.join(BENCH, "projects.json")),
        project = cfg.projects.find((p) => p.name === name);
    requireValue(project, `unknown benchmark project: ${name}`);
    const work = path.resolve(a["work-dir"]);
    run(["node", path.join(BENCH, "bench.mjs"), "prepare", "--projects", name, "--work-dir", work], {
        cwd: ROOT,
        inherit: true,
    });
    const cwd = project.suite_dir ? path.join(work, "suite", project.suite_dir) : path.join(work, "solutions", name),
        root = fs.realpathSync(cwd),
        files = lintFiles(path.resolve(a.tsrs), cwd, path.resolve(cwd, project.project), root);
    requireValue(files.length, `${name}: the project has no files to lint`);
    const request = path.join(out, `${name}.request.json`);
    fs.writeFileSync(
        request,
        JSON.stringify({
            version: 2,
            configs: [{ file_paths: files, rules: RULES.map((rule) => ({ name: rule })) }],
            source_overrides: {},
            report_syntactic: false,
            report_semantic: false,
        }),
    );
    const binaries = { main: path.resolve(a.base), pr: path.resolve(a.head) },
        envs = {},
        result = { project: name, files: files.length, modes: {} };
    for (const [mode, fast] of Object.entries(MODES)) {
        const { env } = cleanEnvironment();
        if (fast) env.TSRS_FAST = fast;
        else delete env.TSRS_FAST;
        envs[mode] = env;
        // Also the warmup for the timed runs.
        const diagnostics = Object.fromEntries(
            Object.entries(binaries).map(([side, binary]) => [
                side,
                lintOnce(binary, request, cwd, env, path.join(out, `${name}.${mode}.${side}.out`)),
            ]),
        );
        result.modes[mode] = { diagnostics: compareDiagnostics(diagnostics.main, diagnostics.pr, root), samples: [] };
    }
    for (let i = 0; i < a.runs; i++) {
        for (const mode of Object.keys(MODES)) {
            const pair = {};
            for (const side of i % 2 ? ["pr", "main"] : ["main", "pr"])
                pair[side] = sample(hf, binaries[side], request, cwd, envs[mode], path.join(out, `${name}.${mode}.${side}.${i}.json`));
            result.modes[mode].samples.push(pair);
            console.log(`${name} ${mode} ${i}: main ${pair.main.wall_s.toFixed(2)} s, PR ${pair.pr.wall_s.toFixed(2)} s`);
        }
    }
    return result;
}

async function measure(a) {
    requireValue(Number.isInteger(a.runs) && a.runs >= 2, "at least two timing pairs are required");
    const out = path.resolve(a.out);
    fs.mkdirSync(out, { recursive: true });
    const hf = await hyperfine(path.resolve(a["work-dir"])),
        projects = {};
    for (const name of a.projects.length ? a.projects : LINT_PROJECTS) {
        try {
            projects[name] = await measureProject(name, a, hf, out);
        } catch (error) {
            console.error(`${name}: ${error.message}`);
            projects[name] = { project: name, error: error.message.split("\n")[0] };
            process.exitCode = 1;
        }
    }
    writeJSON(path.join(out, "lint.json"), { runs: a.runs, rules: RULES.length, projects });
}

// The tsrs-lint section of the PR benchmark comment. `ran` is the build job's decision ("true" or "false").
export function renderLint(ran, results) {
    if (ran === "false")
        return [
            "#### tsrs-lint",
            "",
            "The main and PR `tsrs-lint` binaries are byte-for-byte identical. No lint performance difference to measure.",
            "",
        ];
    if (ran !== "true") return [];
    let data;
    try {
        data = json(results);
    } catch {
        return ["#### tsrs-lint", "", "Lint measurements are missing. See the workflow run for errors.", ""];
    }
    const pct = (ratio) => {
            const value = Number(((ratio - 1) * 100).toFixed(1));
            return `${value > 0 ? "+" : ""}${value.toFixed(1)}%`;
        },
        modeName = (mode) => (MODES[mode] ? `TSRS_FAST=${MODES[mode]}` : "default"),
        lines = [
            "#### tsrs-lint",
            "",
            `Release \`tsrs-lint\` linting each project's source files with all ${data.rules} rules. After a warmup run of each binary, alternating main/PR pairs (${data.runs} per mode) measure wall time, CPU time and peak RSS with TSRS_FAST unset and with TSRS_FAST=1.`,
            "",
            "| Project | Mode | Wall time Δ | CPU time Δ | Peak RSS Δ | Diagnostics (main → PR) |",
            "| --- | --- | ---: | ---: | ---: | --- |",
        ],
        measured = [],
        differences = [];
    for (const [name, project] of Object.entries(data.projects ?? {})) {
        if (!project?.modes) {
            lines.push(`| ${name} | — | failed | — | — | — |`);
            continue;
        }
        for (const [mode, { diagnostics: d, samples }] of Object.entries(project.modes)) {
            const ratio = (key) => median(samples.map((p) => p.pr[key] / p.main[key])),
                med = (side, key) => median(samples.map((p) => p[side][key])),
                same = !d.only_main && !d.only_pr;
            lines.push(
                `| ${name} | ${modeName(mode)} | ${pct(ratio("wall_s"))} | ${pct(ratio("cpu_s"))} | ${pct(med("pr", "peak_rss_bytes") / med("main", "peak_rss_bytes"))} | ${same ? `${d.main}, identical` : `${d.main} → ${d.pr}, ${d.only_main} only on main, ${d.only_pr} only on PR`} |`,
            );
            measured.push(
                `| ${name} | ${modeName(mode)} | ${med("main", "wall_s").toFixed(3)} → ${med("pr", "wall_s").toFixed(3)} | ${med("main", "cpu_s").toFixed(3)} → ${med("pr", "cpu_s").toFixed(3)} | ${(med("main", "peak_rss_bytes") / 2 ** 20).toFixed(1)} → ${(med("pr", "peak_rss_bytes") / 2 ** 20).toFixed(1)} |`,
            );
            if (!same)
                differences.push(
                    `${name} (${modeName(mode)}):`,
                    ...d.first_only_main.map((label) => `- main only: \`${label}\``),
                    ...d.first_only_pr.map((label) => `- PR only: \`${label}\``),
                );
        }
    }
    lines.push(
        "",
        "Negative means the PR used less. Wall and CPU time deltas are medians of paired PR/main ratios; peak RSS compares medians.",
    );
    if (differences.length) lines.push("", "Diagnostics differed (first few of each):", "", ...differences);
    if (measured.length)
        lines.push(
            "",
            "<details>",
            "<summary>Measured lint medians (main → PR)</summary>",
            "",
            "| Project | Mode | Wall time (s) | CPU time (s) | Peak RSS (MiB) |",
            "| --- | --- | ---: | ---: | ---: |",
            ...measured,
            "",
            "</details>",
        );
    return lines.concat("");
}

if (isMain(import.meta.url))
    main(async () => {
        const a = args(
            "Compare main and PR tsrs-lint on the pinned lint projects.",
            { base: null, head: null, tsrs: null, out: null, projects: [], runs: 5, "work-dir": path.join(BENCH, ".work") },
            process.argv.slice(2),
        );
        required(a, "base", "head", "tsrs", "out");
        await measure(a);
    });
