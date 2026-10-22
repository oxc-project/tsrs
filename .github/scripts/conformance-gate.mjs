#!/usr/bin/env node
// Fingerprint named outcomes so new passes cannot hide regressions or additional skips.
// --check <summary.json> checks an existing full-suite run; --update records a fresh run
// after the complete pass/skip changes have been reviewed (CONTRIBUTING.md).
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const snapshotPath = path.join(root, ".github/conformance-baselines.json");
const extensions = { types: "types", symbols: "symbols", js: "js", jsmap: "js.map", sourcemap: "sourcemap.txt" };

function fingerprint(names) {
    return { count: names.length, sha256: createHash("sha256").update(names.slice().sort().join("\n") + "\n").digest("hex") };
}

function sameFingerprint(actual, expected) {
    return actual.count === expected?.count && actual.sha256 === expected.sha256;
}

function run(command, args) {
    const result = spawnSync(command, args, { cwd: root, encoding: "utf8" });
    if (result.error) throw result.error;
    if (result.status !== 0) throw new Error(`${command} exited with ${result.status ?? result.signal}: ${result.stderr ?? ""}`);
    return result.stdout.trim();
}

function readOutcomes(summary, baseline) {
    const classes = { pass: [], skip: [], codes: [], fail: [], crash: [], timeout: [] };
    const seen = new Set();
    if (!Array.isArray(summary.tests) || summary.tests.length === 0) throw new Error("missing full-suite results");
    for (const test of summary.tests) {
        const outcome = baseline === "errors" ? test.class : test[baseline];
        if (typeof test.id !== "string" || !test.id || seen.has(test.id) || typeof outcome !== "string" || !Object.hasOwn(classes, outcome)) {
            throw new Error(`invalid or missing ${baseline} result for ${test.id}`);
        }
        seen.add(test.id);
        classes[outcome].push(test.id);
    }
    return Object.fromEntries(Object.entries(classes).map(([kind, names]) => [kind, names.sort()]));
}

function fourslashOutcomes(file) {
    const outcomes = new Map();
    let packageResult;
    for (const line of fs.readFileSync(file, "utf8").trim().split("\n")) {
        const event = JSON.parse(line);
        if (!["pass", "fail", "skip"].includes(event.Action)) continue;
        if (event.Test) outcomes.set(event.Test, event.Action);
        else packageResult = event.Action;
    }
    if (packageResult !== "pass" || [...outcomes.values()].includes("fail")) throw new Error("incomplete or failing Fourslash suite");
    const children = new Map();
    for (const name of outcomes.keys()) {
        const separator = name.lastIndexOf("/");
        if (separator < 0) continue;
        const parent = name.slice(0, separator);
        if (!children.has(parent)) children.set(parent, []);
        children.get(parent).push(name);
    }
    // Match summarize.py: parents with only skipped descendants do not count as coverage.
    for (const name of [...outcomes.keys()].sort((a, b) => b.split("/").length - a.split("/").length)) {
        if (outcomes.get(name) === "pass" && children.has(name) && children.get(name).every(child => outcomes.get(child) === "skip")) {
            outcomes.set(name, "skip");
        }
    }
    return Object.fromEntries(["pass", "skip"].map(kind => [kind, fingerprint([...outcomes].filter(([name, outcome]) => !name.includes("/") && outcome === kind).map(([name]) => name))]));
}

function main() {
    const args = process.argv.slice(2);
    const fourslash = args[0] === "--fourslash" && (args.length === 2 || (args.length === 3 && args[2] === "--update"));
    const update = (args.length === 1 && args[0] === "--update") || (fourslash && args[2] === "--update");
    const check = args.length === 2 && args[0] === "--check";
    if (args.length && !update && !check && !fourslash) throw new Error("usage: conformance-gate.sh [--check <summary.json> | --update | --fourslash <results.jsonl> [--update]]");
    const tokens = (process.env.BASELINES ?? "types,symbols,js,jsmap,sourcemap").split(",").map(value => value.trim()).filter(Boolean);
    const requested = new Set(["errors"]);
    for (const token of tokens) {
        if (token === "all") {
            requested.add("types");
            requested.add("symbols");
        } else if (token !== "errors") {
            if (!Object.hasOwn(extensions, token)) throw new Error(`unknown baseline ${token}`);
            requested.add(extensions[token]);
        }
    }
    const mode = ["js", "js.map", "sourcemap.txt"].some(kind => requested.has(kind)) ? "emit" : "check";
    const cargo = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8");
    const commit = cargo.match(/\[workspace\.metadata\.typescript\][^[]*?commit\s*=\s*"([0-9a-f]{40})"/s)?.[1];
    if (!commit) throw new Error("missing TypeScript source pin");
    const testdata = path.resolve(process.env.TSRS_TESTDATA ?? path.join(root, "ts-ref/tsc/testdata"));
    if (run("git", ["-C", testdata, "rev-parse", "HEAD"]) !== commit) throw new Error("testdata checkout does not match the TypeScript source pin");
    const snapshot = JSON.parse(fs.readFileSync(snapshotPath, "utf8"));
    if (!update && snapshot.typescriptCommit !== commit) throw new Error("conformance snapshot does not match the TypeScript source pin");
    if (fourslash) {
        const actual = fourslashOutcomes(args[1]);
        if (update) {
            if (snapshot.typescriptCommit !== commit) throw new Error("update the compiler snapshot to the TypeScript source pin first");
            snapshot.fourslash = actual;
            fs.writeFileSync(snapshotPath, JSON.stringify(snapshot, null, 2) + "\n");
        } else if (!["pass", "skip"].every(kind => sameFingerprint(actual[kind], snapshot.fourslash?.[kind]))) {
            throw new Error(`Fourslash named pass/skip sets changed: pass=${actual.pass.count}, skip=${actual.skip.count}`);
        }
        console.log(`fourslash ${update ? "snapshot updated" : "gate passed"}: pass=${actual.pass.count}; skip=${actual.skip.count}`);
        return;
    }

    let summaryPath = check ? path.resolve(args[1]) : undefined;
    if (!check) {
        const results = path.resolve(process.env.TSRS_TEST_RESULTS ?? path.join(root, "target/test-results"));
        fs.mkdirSync(results, { recursive: true });
        const temporary = fs.mkdtempSync(path.join(results, ".conformance-"));
        summaryPath = path.join(results, "summary.json");
        try {
            // A fresh JSON path prevents a missing/partial run from reusing stale outcomes.
            const fresh = path.join(temporary, "summary.json");
            const binary = process.env.TSRS_TEST ?? path.join(root, "target/debug/tsrs-test");
            const result = spawnSync(binary, ["run", "--suite", "all", "--baselines", tokens.join(","), "--timeout", process.env.TEST_TIMEOUT ?? "120", "--json", fresh], {
                cwd: root, stdio: "inherit", env: { ...process.env, TSRS_TEST_RESULTS: results, TSRS_TESTDATA: testdata },
            });
            const produced = fs.existsSync(fresh);
            if (produced) fs.renameSync(fresh, summaryPath);
            if (result.error) throw result.error;
            if (result.status !== 0) throw new Error(`tsrs-test exited with ${result.status ?? result.signal}`);
            if (!produced) throw new Error("tsrs-test did not write a summary");
        } finally {
            fs.rmSync(temporary, { recursive: true, force: true });
        }
    }
    const summary = JSON.parse(fs.readFileSync(summaryPath, "utf8"));
    const failures = [];
    const totals = [];
    for (const baseline of requested) {
        const expected = snapshot.modes[mode][baseline];
        if (!expected) throw new Error(`no ${mode}/${baseline} snapshot`);
        const outcomes = readOutcomes(summary, baseline);
        for (const kind of ["codes", "fail", "crash", "timeout"]) {
            const allowed = expected[kind] ?? [];
            const unexpected = outcomes[kind].filter(id => !allowed.includes(id));
            if (unexpected.length) failures.push(`${baseline} ${kind}: ${unexpected.slice(0, 20).join(", ")}`);
        }
        const actual = { pass: fingerprint(outcomes.pass), skip: fingerprint(outcomes.skip), codes: outcomes.codes, fail: outcomes.fail };
        if (!update) {
            for (const kind of ["pass", "skip", "codes", "fail"]) {
                const matches = kind === "pass" || kind === "skip"
                    ? sameFingerprint(actual[kind], expected[kind])
                    : JSON.stringify(actual[kind]) === JSON.stringify(expected[kind]);
                if (!matches) {
                    failures.push(`${mode}/${baseline} ${kind} set changed (expected ${expected[kind].count ?? expected[kind].length}, got ${outcomes[kind].length})`);
                }
            }
        }
        snapshot.modes[mode][baseline] = actual;
        totals.push(`${baseline}=${outcomes.pass.length}`);
    }
    if (failures.length) throw new Error(failures.join("\n"));
    if (update) {
        snapshot.typescriptCommit = commit;
        fs.writeFileSync(snapshotPath, JSON.stringify(snapshot, null, 2) + "\n");
    }
    console.log(`conformance ${mode} ${update ? "snapshot updated" : "gate passed"}: ${totals.join("; ")}`);
}

try {
    main();
} catch (error) {
    console.error(`::error::${error.message}`);
    process.exitCode = 1;
}
