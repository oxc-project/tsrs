#!/usr/bin/env node
// Compare the Rust headless binary with exported, pinned tsgolint cases.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { fork, spawnSync } from "node:child_process";
import { isDeepStrictEqual, parseArgs } from "node:util";
import { main } from "../lib/cli.mjs";

const USAGE = "usage: run-baselines.mjs [-h] [--rule RULE] [--jobs JOBS] binary baselines";
const DESCRIPTION = "Compare the Rust headless binary with exported, pinned tsgolint cases.";

class CaseError extends Error {}

function check(ok, message) {
    if (!ok) throw new CaseError(message);
}

function usageError(message) {
    console.error(`${USAGE}\nrun-baselines.mjs: error: ${message}`);
    process.exit(2);
}

function frames(data) {
    const result = [];
    let offset = 0;
    while (offset < data.length) {
        check(data.length - offset >= 5, "truncated frame header");
        const length = data.readUInt32LE(offset),
            kind = data[offset + 4];
        offset += 5;
        check(data.length - offset >= length, "truncated frame payload");
        const payload = JSON.parse(data.subarray(offset, offset + length).toString("utf8"));
        offset += length;
        if (kind === 0) throw new CaseError(String(payload.error));
        if (kind === 1) result.push(payload);
        else check(kind === 2, `unknown frame type ${kind}`);
    }
    return result;
}

const decoder = new TextDecoder("utf-8", { fatal: true });

function applyFixes(code, diagnostics) {
    const groups = diagnostics
        .filter((diagnostic) => diagnostic.fixes?.length)
        .map((diagnostic) =>
            [...diagnostic.fixes].sort((a, b) => a.range.pos - b.range.pos || a.range.end - b.range.end),
        );
    groups.sort((a, b) => a[0].range.pos - b[0].range.pos || a.at(-1).range.end - b.at(-1).range.end);
    const data = Buffer.from(code, "utf8"),
        output = [];
    let last = 0,
        fixed = false;
    for (const group of groups) {
        if (last > group[0].range.pos) continue;
        for (const fix of group) {
            const { pos: start, end } = fix.range;
            check(last <= start && start <= end && end <= data.length, "invalid fix range");
            output.push(data.subarray(last, start), Buffer.from(fix.text, "utf8"));
            last = end;
            fixed = true;
        }
    }
    output.push(data.subarray(last));
    try {
        return [decoder.decode(Buffer.concat(output)), fixed];
    } catch (error) {
        throw new CaseError(error.message);
    }
}

function* configs(dir) {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        const file = path.join(dir, entry.name);
        if (entry.isDirectory()) yield* configs(file);
        else if (/^tsconfig.*\.json$/.test(entry.name)) yield file;
    }
}

function selectConfig(root, selected) {
    root = fs.realpathSync(root);
    const base = path.join(root, "tsconfig.json"),
        preserved = path.join(root, "tsconfig.tsrs-original.json");
    fs.renameSync(base, preserved);
    for (const config of configs(root)) {
        const parent = path.dirname(config);
        const text = fs.readFileSync(config, "utf8").replace(/"extends"\s*:\s*"([^"\n]+)"/g, (match, target) => {
            if (path.resolve(parent, target) !== base) return match;
            const relative = path.relative(parent, preserved).split(path.sep).join("/");
            return `"extends": ${JSON.stringify(`./${relative}`)}`;
        });
        fs.writeFileSync(config, text);
    }
    if (selected === "tsconfig.json") selected = path.basename(preserved);
    const selectedPath = path.join(root, selected);
    check(path.dirname(selectedPath) === root, "baseline configuration must be in fixture root");
    fs.writeFileSync(base, fs.readFileSync(selectedPath, "utf8"));
}

function zipStrict(left, right) {
    check(left.length === right.length, "zip() arguments have different lengths");
    return left.map((value, index) => [value, right[index]]);
}

function runCase(binary, fixtures, rule, suiteConfig, testCase) {
    const expected = testCase.Diagnostics;
    check(Array.isArray(expected), "missing exported diagnostics");
    const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-lint-case-")));
    try {
        fs.cpSync(fixtures, root, { recursive: true, dereference: true });
        const dependencies = path.join(path.dirname(fixtures), "dependencies");
        if (fs.existsSync(dependencies))
            fs.cpSync(dependencies, path.join(root, "node_modules"), { recursive: true, dereference: true });
        const filename = testCase.FileName || (testCase.Tsx ? "react.tsx" : "file.ts");
        const file = path.join(root, filename);
        selectConfig(root, testCase.TSConfig || suiteConfig);
        const overrides = Object.fromEntries(
            Object.entries(testCase.Files || {}).map(([name, text]) => [path.join(root, name), text]),
        );
        const payload = {
            version: 2,
            configs: [{ file_paths: [file], rules: [{ name: rule, options: testCase.Options }] }],
            source_overrides: overrides,
        };
        const lint = (code) => {
            payload.source_overrides[file] = code;
            const result = spawnSync(binary, ["headless", "-fix", "-fix-suggestions"], {
                input: JSON.stringify(payload),
                cwd: root,
                maxBuffer: 1024 * 1024 * 1024,
            });
            if (result.error) throw result.error;
            const diagnostics = frames(result.stdout);
            check(result.status === 0, result.stderr.toString("utf8"));
            for (const diagnostic of diagnostics) {
                const reported = diagnostic.file_path;
                if (reported !== undefined && reported !== null) {
                    const relative = path.relative(root, reported);
                    check(
                        path.isAbsolute(reported) && relative !== ".." && !relative.startsWith(`..${path.sep}`),
                        `${JSON.stringify(reported)} is not in the subpath of ${JSON.stringify(root)}`,
                    );
                    diagnostic.file_path = relative.split(path.sep).join("/") || ".";
                }
            }
            return diagnostics;
        };
        let diagnostics = lint(testCase.Code);
        check(
            isDeepStrictEqual(diagnostics, expected),
            `diagnostics differ\nexpected ${JSON.stringify(expected)}\nactual ${JSON.stringify(diagnostics)}`,
        );
        const outputs = [];
        let code = testCase.Code;
        for (let pass = 0; pass < 10; pass++) {
            let fixed;
            [code, fixed] = applyFixes(code, diagnostics);
            if (!fixed) break;
            outputs.push(code);
            diagnostics = lint(code);
        }
        check(isDeepStrictEqual(outputs, testCase.Output || []), `fix outputs differ: ${JSON.stringify(outputs)}`);
        for (const [diagnostic, error] of zipStrict(expected, testCase.Errors || [])) {
            for (const [suggestion, expectedSuggestion] of zipStrict(
                diagnostic.suggestions || [],
                error.Suggestions || [],
            )) {
                const [output] = applyFixes(testCase.Code, [suggestion]);
                check(output === expectedSuggestion.Output, "suggestion output differs");
            }
        }
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
}

function baselineFiles(root, rule) {
    const directories = rule
        ? [rule]
        : fs
              .readdirSync(root, { withFileTypes: true })
              .filter((entry) => entry.isDirectory())
              .map((entry) => entry.name);
    return directories
        .sort((a, b) => (a < b ? -1 : a > b ? 1 : 0))
        .map((directory) => path.join(root, directory, "baseline.json"))
        .filter((file) => fs.existsSync(file) && fs.statSync(file).isFile());
}

function loadCases(baselines) {
    const cases = [],
        suites = [];
    let skipped = 0;
    for (const source of baselines) {
        const baseline = JSON.parse(fs.readFileSync(source, "utf8"));
        for (const [test, suite] of Object.entries(baseline.Suites)) {
            const only = ["Valid", "Invalid"].some((group) => suite[group].some((testCase) => testCase.Only));
            for (const group of ["Valid", "Invalid"]) {
                for (const [index, testCase] of suite[group].entries()) {
                    if (testCase.Skip || (only && !testCase.Only)) skipped++;
                    else
                        cases.push({
                            rule: baseline.Name,
                            name: `${test}_${group.toLowerCase()}-${index}`,
                            suiteConfig: suite.TSConfig,
                            testCase,
                        });
                }
            }
            suites.push(`${baseline.Name}: suite ${test} passed`);
        }
    }
    return { cases, suites, skipped };
}

const WORKER = "--case-worker";

// Each case copies the fixtures and runs the binary synchronously, so cases run in worker processes. Spawning the
// binary from worker threads of one process scaled worse on Linux.
function serveCases(binary, fixtures) {
    process.on("message", ({ index, rule, name, suiteConfig, testCase }) => {
        try {
            runCase(binary, fixtures, rule, suiteConfig, testCase);
            process.send({ index });
        } catch (error) {
            process.send({ index, error: `${rule}/${name}: ${error.message}` });
        }
    });
}

// Cases are dispatched in order and dispatch stops at the first failure, so every case before the reported one ran
// and passed: the report matches a sequential run's.
async function runCases(binary, fixtures, cases, jobs) {
    let next = 0,
        failure;
    const runWorker = () =>
        new Promise((resolve) => {
            const worker = fork(fileURLToPath(import.meta.url), [WORKER, binary, fixtures], {
                stdio: ["ignore", "inherit", "inherit", "ipc"],
            });
            let done = false;
            const settle = (index, error) => {
                if (done) return;
                if (error !== undefined && (!failure || index < failure.index)) failure = { index, error };
                if (failure || next >= cases.length) {
                    done = true;
                    if (worker.connected) worker.disconnect();
                    resolve();
                } else worker.send({ index: next, ...cases[next++] });
            };
            worker.on("message", ({ index, error }) => settle(index, error));
            worker.on("error", (error) => settle(cases.length, error.message));
            worker.on("exit", (code, signal) => settle(cases.length, `case worker exited with ${signal ?? code}`));
            settle();
        });
    await Promise.all(Array.from({ length: Math.min(jobs, cases.length) }, runWorker));
    if (failure) throw new Error(failure.error);
}

function cli() {
    let parsed;
    try {
        parsed = parseArgs({
            options: {
                help: { type: "boolean", short: "h" },
                rule: { type: "string" },
                jobs: { type: "string" },
            },
            allowPositionals: true,
        });
    } catch (error) {
        usageError(error.message);
    }
    const { values, positionals } = parsed;
    if (values.help) {
        console.log(
            [
                USAGE,
                "",
                DESCRIPTION,
                "",
                "positional arguments:",
                "  binary",
                "  baselines",
                "",
                "options:",
                "  -h, --help   show this help message and exit",
                "  --rule RULE",
                "  --jobs JOBS  cases to run at once (default: available parallelism, at most 8)",
            ].join("\n"),
        );
        return;
    }
    if (positionals.length < 2) usageError("the following arguments are required: binary, baselines");
    if (positionals.length > 2) usageError(`unrecognized arguments: ${positionals.slice(2).join(" ")}`);
    // More workers stop helping on Linux and slow macOS down: 18 workers took 370 s on APFS, 8 took 155 s.
    const jobs = values.jobs === undefined ? Math.min(os.availableParallelism(), 8) : Number(values.jobs);
    if (!Number.isInteger(jobs) || jobs < 1) usageError("--jobs must be a positive integer");
    const root = fs.realpathSync(positionals[1]),
        binary = path.resolve(positionals[0]);
    const baselines = baselineFiles(root, values.rule);
    if (!baselines.length) usageError("no baseline suites found");
    const { cases, suites, skipped } = loadCases(baselines);
    return runCases(binary, path.join(root, "fixtures"), cases, jobs).then(() => {
        for (const suite of suites) console.log(suite);
        console.log(`${cases.length} cases passed; ${skipped} upstream cases skipped`);
    });
}

if (process.argv[2] === WORKER) serveCases(process.argv[3], process.argv[4]);
else main(cli);
