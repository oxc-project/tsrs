#!/usr/bin/env node
// Benchmark headless helpers on one payload, with and without full type checking (macOS).
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { performance } from "node:perf_hooks";
import { isMain, runAsync } from "../tools/lib/cli.mjs";

const CLEARED_ENV = [
    "GOMAXPROCS",
    "RAYON_NUM_THREADS",
    "TSRS_CHECKERS",
    "TSRS_HISTORY",
    "TS_TEST_PROGRAM_SINGLE_THREADED",
    "OXLINT_TSGOLINT_DANGEROUSLY_SUPPRESS_PROGRAM_DIAGNOSTICS",
    "OXLINT_TSGOLINT_CHECKER_SCHEDULING",
];

export const sha256 = (data) => crypto.createHash("sha256").update(data).digest("hex");
export const digest = (file) => sha256(fs.readFileSync(file));

function sortKeys(value) {
    if (Array.isArray(value)) return value.map(sortKeys);
    if (value === null || typeof value !== "object") return value;
    const compare = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
    return Object.fromEntries(
        Object.keys(value)
            .sort(compare)
            .map((key) => [key, sortKeys(value[key])]),
    );
}

// Python's json.dumps(value, sort_keys=True, separators=(",", ":")), so digests match earlier runs.
function canonicalJSON(value) {
    return JSON.stringify(sortKeys(value)).replace(
        /[\u007f-\uffff]/g,
        (char) => `\\u${char.charCodeAt(0).toString(16).padStart(4, "0")}`,
    );
}

const byKey = (a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : a[1] - b[1]);

export function diagnosticDigest(diagnostics, label) {
    const records = new Map(),
        counts = new Map();
    for (const diagnostic of diagnostics) {
        const hash = sha256(canonicalJSON(diagnostic));
        records.set(hash, (records.get(hash) ?? 0) + 1);
        const key = label(diagnostic);
        counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return {
        count: diagnostics.length,
        counts: Object.fromEntries([...counts].sort(byKey)),
        sha256: sha256(JSON.stringify([...records].sort(byKey))),
    };
}

function summarize(file) {
    const data = fs.readFileSync(file),
        diagnostics = [];
    let offset = 0;
    while (offset < data.length) {
        if (data.length - offset < 5) throw new Error("short frame header");
        const length = data.readUInt32LE(offset),
            kind = data[offset + 4];
        offset += 5;
        if (data.length - offset < length) throw new Error("short frame body");
        const value = JSON.parse(data.subarray(offset, offset + length).toString("utf8"));
        offset += length;
        if (kind === 0) throw new Error(`helper error: ${JSON.stringify(value)}`);
        if (kind === 1) diagnostics.push(value);
    }
    const result = diagnosticDigest(diagnostics, (diagnostic) => diagnostic.rule || "<compiler>");
    return {
        diagnostics: result.count,
        diagnostics_by_rule: result.counts,
        diagnostic_multiset_sha256: result.sha256,
    };
}

export function counters(file) {
    const text = fs.readFileSync(file, "utf8"),
        result = {};
    for (const [key, pattern] of [
        ["rss_bytes", /(\d+)\s+maximum resident set size/],
        ["instructions", /(\d+)\s+instructions retired/],
        ["cpu_cycles", /(\d+)\s+cycles elapsed/],
    ]) {
        const match = text.match(pattern);
        if (match) result[key] = Number(match[1]);
    }
    const match = text.match(/([\d.]+)\s+real\s+([\d.]+)\s+user\s+([\d.]+)\s+sys/);
    if (match) Object.assign(result, { time_real: Number(match[1]), user: Number(match[2]), sys: Number(match[3]) });
    if (!("instructions" in result) || !("rss_bytes" in result))
        throw new Error(`missing macOS time counters in ${file}`);
    return result;
}

export function parseCommandLine(spec, usage, description, argv = process.argv.slice(2)) {
    const name = path.basename(process.argv[1]);
    const fail = (message) => {
        console.error(`${usage}\n${name}: error: ${message}`);
        process.exit(2);
    };
    const values = {};
    for (let index = 0; index < argv.length; index++) {
        const argument = argv[index];
        if (argument === "-h" || argument === "--help") {
            console.log(`${usage}\n\n${description}\n\noptions:\n${Object.keys(spec)
                .map((key) => `  --${key}`)
                .join("\n")}`);
            process.exit(0);
        }
        const [flag, inline] = argument.startsWith("--") ? argument.slice(2).split(/=(.*)/s) : [];
        const option = Object.hasOwn(spec, flag ?? "") ? spec[flag] : undefined;
        if (!option) fail(`unrecognized arguments: ${argument}`);
        let items = inline !== undefined ? [inline] : [];
        if (inline === undefined) {
            while (index + 1 < argv.length && !argv[index + 1].startsWith("--")) {
                items.push(argv[++index]);
                if (!option.many) break;
            }
        }
        if (!items.length)
            fail(`argument --${flag}: expected ${option.many ? "at least one argument" : "one argument"}`);
        if (option.type === "int") {
            items = items.map((item) => {
                if (!/^[-+]?\d+$/.test(item.trim())) fail(`argument --${flag}: invalid int value: '${item}'`);
                return Number(item);
            });
        }
        for (const item of items)
            if (option.choices && !option.choices.includes(item))
                fail(
                    `argument --${flag}: invalid choice: '${item}' ` +
                        `(choose from ${option.choices.map((choice) => `'${choice}'`).join(", ")})`,
                );
        if (option.excludes && values[option.excludes] !== undefined)
            fail(`argument --${flag}: not allowed with argument --${option.excludes}`);
        if (option.append) (values[flag] ??= []).push(...items);
        else values[flag] = option.many ? items : items[0];
    }
    const missing = Object.entries(spec)
        .filter(([key, option]) => option.required && values[key] === undefined)
        .map(([key]) => `--${key}`);
    if (missing.length) fail(`the following arguments are required: ${missing.join(", ")}`);
    for (const [key, option] of Object.entries(spec)) values[key] ??= option.default;
    return { values, fail };
}

export function parseHelpers(specs, fail) {
    const helpers = new Map();
    for (const helper of specs) {
        const separator = helper.indexOf("=");
        const name = separator < 0 ? "" : helper.slice(0, separator);
        if (separator < 0 || !/^[a-zA-Z0-9_-]+$/.test(name) || helpers.has(name))
            fail("--helper requires a unique NAME=PATH");
        helpers.set(name, fs.realpathSync(helper.slice(separator + 1)));
    }
    return helpers;
}

export function createOutput(directory) {
    const output = path.resolve(directory);
    if (fs.existsSync(output)) throw new Error(`${output} already exists`);
    fs.mkdirSync(output, { recursive: true });
    return output;
}

export const resolvePath = (file) => (fs.existsSync(file) ? fs.realpathSync(file) : path.resolve(file));

export function cleanEnv() {
    const env = { ...process.env };
    for (const key of CLEARED_ENV) delete env[key];
    return env;
}

// Like Python's start_new_session plus killpg: interruption also kills the timed helper's process group.
export async function timed(argv, options) {
    let child;
    const interrupt = () => {
        try {
            if (child?.pid) process.kill(-child.pid, "SIGKILL");
        } catch (error) {
            if (error.code !== "ESRCH") throw error;
        }
        process.exit(130);
    };
    process.once("SIGINT", interrupt);
    try {
        const started = performance.now();
        const { code } = await runAsync(argv, {
            ...options,
            check: false,
            timeout: 900,
            onSpawn: (spawned) => {
                child = spawned;
            },
        });
        return { code, wall: (performance.now() - started) / 1000 };
    } finally {
        process.removeListener("SIGINT", interrupt);
    }
}

export function appendSample(output, sample) {
    fs.appendFileSync(path.join(output, "samples.jsonl"), JSON.stringify(sample) + "\n");
}

async function main() {
    const usage =
        "usage: tsgolint.mjs [-h] --payload PAYLOAD --cwd CWD --helper NAME=PATH --output OUTPUT [--runs RUNS] " +
        "[--warmups WARMUPS] [--modes {default,single} ... | --threads THREADS ...] " +
        "[--variants {lint,lint-typecheck,typecheck,setup} ...]";
    const { values: args, fail } = parseCommandLine(
        {
            payload: { required: true },
            cwd: { required: true },
            helper: { required: true, append: true },
            output: { required: true },
            runs: { type: "int", default: 3 },
            warmups: { type: "int", default: 1 },
            modes: { many: true, choices: ["default", "single"], default: ["default", "single"], excludes: "threads" },
            threads: { many: true, type: "int", excludes: "modes" },
            variants: {
                many: true,
                choices: ["lint", "lint-typecheck", "typecheck", "setup"],
                default: ["lint", "lint-typecheck"],
            },
        },
        usage,
        "Benchmark headless helpers on one payload, with and without full type checking (macOS).",
    );
    if (args.runs < 1 || args.warmups < 0) fail("runs must be positive and warmups nonnegative");
    if (
        args.threads &&
        (args.threads.some((n) => n < 1 || n > 256) || new Set(args.threads).size !== args.threads.length)
    )
        fail("threads must be unique counts between 1 and 256");
    const configurations = args.threads
        ? args.threads.map((n) => [`threads-${n}`, n])
        : args.modes.map((mode) => [mode, mode === "single" ? 1 : null]);
    const helpers = parseHelpers(args.helper, fail);
    const output = createOutput(args.output);
    const source = JSON.parse(fs.readFileSync(args.payload, "utf8"));
    if (source.version !== 2) fail("a version 2 payload is required");
    const inputs = new Map();
    for (const variant of args.variants) {
        const payload = structuredClone(source);
        payload.report_semantic = payload.report_syntactic = ["lint-typecheck", "typecheck"].includes(variant);
        if (["typecheck", "setup"].includes(variant)) for (const config of payload.configs) config.rules = [];
        const file = path.join(output, `${variant}.json`);
        fs.writeFileSync(file, JSON.stringify(payload));
        inputs.set(variant, file);
    }
    const metadata = {
        source_payload_sha256: digest(args.payload),
        cwd: resolvePath(args.cwd),
        helpers: Object.fromEntries([...helpers].map(([name, file]) => [name, { path: file, sha256: digest(file) }])),
        variants: Object.fromEntries([...inputs].map(([name, file]) => [name, { sha256: digest(file) }])),
        runs: args.runs,
        warmups: args.warmups,
        modes: configurations.map(([mode]) => mode),
        threads: args.threads ?? null,
        single_mode: "GOMAXPROCS=1 RAYON_NUM_THREADS=1; Go may retain multiple checker states",
    };
    fs.writeFileSync(path.join(output, "metadata.json"), JSON.stringify(metadata, null, 2) + "\n");
    for (const [variant, payload] of inputs) {
        for (let repetition = -args.warmups; repetition < args.runs; repetition++) {
            const offset = Math.max(repetition, 0) % configurations.length;
            for (const [mode, threads] of [...configurations.slice(offset), ...configurations.slice(0, offset)]) {
                const order = [...helpers.keys()];
                if (repetition >= 0 && repetition % 2) order.reverse();
                for (const name of order) {
                    const label = `${variant}-${mode}-${repetition}-${name}`;
                    const env = cleanEnv();
                    if (threads !== null) {
                        env.RAYON_NUM_THREADS = String(threads);
                        env.GOMAXPROCS = String(threads);
                    }
                    const stdin = fs.openSync(payload, "r"),
                        stdout = fs.openSync(path.join(output, `${label}.out`), "w"),
                        stderr = fs.openSync(path.join(output, `${label}.err`), "w");
                    const time = path.join(output, `${label}.time`);
                    let status, wall;
                    try {
                        ({ code: status, wall } = await timed(
                            ["/usr/bin/time", "-l", "-o", time, helpers.get(name), "headless"],
                            { cwd: args.cwd, env, stdio: [stdin, stdout, stderr] },
                        ));
                    } finally {
                        for (const fd of [stdin, stdout, stderr]) fs.closeSync(fd);
                    }
                    if (status)
                        throw new Error(`${label} exited ${status}; inspect ${path.join(output, `${label}.err`)}`);
                    const sample = {
                        helper: name,
                        variant,
                        mode,
                        rep: repetition,
                        threads,
                        warmup: repetition < 0,
                        wall_s: wall,
                        exit_code: status,
                        ...counters(time),
                        ...summarize(path.join(output, `${label}.out`)),
                    };
                    appendSample(output, sample);
                    const rss = (sample.rss_bytes / 2 ** 20).toFixed(1);
                    console.log(`${label}: ${wall.toFixed(3)}s, ${rss} MiB, ${sample.diagnostics} diagnostics`);
                }
            }
        }
    }
}

if (isMain(import.meta.url))
    main().catch((error) => {
        console.error(error.message);
        process.exitCode = 1;
    });
