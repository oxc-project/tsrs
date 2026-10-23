#!/usr/bin/env node
// Mutate a disposable clone, compare diagnostics, and restore files even on failure.
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { createHash } from "node:crypto";
import { ROOT, args, read, write, json, writeJSON, run, runAsync, random, requireValue, main } from "../lib/cli.mjs";
import { pickMutation } from "./mutators.mjs";
const WORK = process.env.TSRS_WORK ?? path.join(os.homedir(), "tsrs-work"),
    OUT = process.env.MUT_OUT ?? path.join(ROOT, "target/scratch/mutate"),
    REF = process.env.MUT_REF ?? path.join(WORK, "bin/tsgo-ref"),
    OURS = process.env.MUT_OURS ?? path.join(ROOT, "target/release/tsrs");
let clone, pristine;
const safe = (root, rel) => {
    const target = path.resolve(root, rel);
    requireValue(target.startsWith(root + path.sep), "mutation path escapes the project");
    return target;
};
function restore() {
    const file = path.join(OUT, "DIRTY");
    if (!fs.existsSync(file)) return 0;
    const rels = [...new Set(read(file).split("\n").filter(Boolean))];
    for (const rel of rels) {
        const source = safe(pristine, rel),
            dest = safe(clone, rel);
        fs.rmSync(dest, { force: true });
        fs.copyFileSync(source, dest);
        requireValue(read(source) === read(dest), "restored file differs");
    }
    fs.rmSync(file);
    return rels.length;
}
function apply(mutations) {
    const groups = new Map();
    for (const m of mutations) {
        if (!groups.has(m.file)) groups.set(m.file, []);
        groups.get(m.file).push(m);
    }
    fs.mkdirSync(OUT, { recursive: true });
    fs.appendFileSync(path.join(OUT, "DIRTY"), [...groups.keys()].join("\n") + "\n");
    for (const [rel, list] of groups) {
        let src = read(safe(pristine, rel));
        requireValue(src === read(safe(clone, rel)), "clone differs from pristine before mutation; run mutate restore");
        const edits = list.map((m) => {
            requireValue(
                Number.isInteger(m.start) && Number.isInteger(m.end) && m.start >= 0 && m.end >= m.start,
                "invalid mutation span",
            );
            if (src.slice(m.start, m.end) === m.orig) return m;
            // Saved Python batches used code-point offsets; new batches use JavaScript UTF-16 offsets.
            const points = Array.from(src);
            requireValue(m.end <= points.length, "mutation span exceeds the file");
            const start = points.slice(0, m.start).join("").length,
                end = points.slice(0, m.end).join("").length;
            return { ...m, start, end };
        });
        let boundary = src.length;
        for (const m of edits.sort((a, b) => b.start - a.start)) {
            requireValue(m.end <= boundary, "mutation spans overlap or exceed the file");
            requireValue(src.slice(m.start, m.end) === m.orig, "mutation original span differs");
            src = src.slice(0, m.start) + m.repl + src.slice(m.end);
            boundary = m.start;
        }
        write(safe(clone, rel), src);
    }
}
export function parseDiags(text) {
    const out = [];
    for (const line of text.split(/\r?\n/)) {
        const m = /^(.*)\((\d+),(\d+)\): error TS(\d+): (.*)$/.exec(line),
            g = /^error TS(\d+): (.*)$/.exec(line);
        if (m) out.push({ file: m[1].replace(/^\.\//, ""), line: +m[2], col: +m[3], code: +m[4], msg: m[5], more: [] });
        else if (g) out.push({ file: "", line: 0, col: 0, code: +g[1], msg: g[2], more: [] });
        else if (out.length && /^[ \t]/.test(line)) out.at(-1).more.push(line);
    }
    return out;
}
const diagKey = (d) => JSON.stringify([d.file, d.line, d.col, d.code]);
function diff(out, mutations) {
    const ref = parseDiags(read(path.join(out, "ref.txt"))),
        ours = parseDiags(read(path.join(out, "ours.txt"))),
        group = (ds) => {
            const result = new Map();
            for (const d of ds) {
                const k = diagKey(d);
                if (!result.has(k)) result.set(k, []);
                result.get(k).push(d);
            }
            return result;
        },
        a = group(ref),
        b = group(ours),
        missing = [],
        extra = [],
        textDiffs = [];
    let placeholder = 0;
    for (const key of new Set([...a.keys(), ...b.keys()])) {
        const left = a.get(key) ?? [],
            right = b.get(key) ?? [],
            decoded = JSON.parse(key),
            muts = mutations.filter((m) => m.file === decoded[0]).map((m) => m.id);
        if (left.length !== right.length)
            (left.length > right.length ? missing : extra).push({
                key: decoded,
                n: Math.abs(left.length - right.length),
                muts,
                msg: (left.length > right.length ? left : right)[0].msg,
            });
        for (let i = 0; i < Math.min(left.length, right.length); i++) {
            const old = [left[i].msg, ...left[i].more].join("\n"),
                current = [right[i].msg, ...right[i].more].join("\n");
            if (old === current) continue;
            if (current.includes("type#") || current.includes("'\"/")) placeholder++;
            else textDiffs.push({ key: decoded, ref: old, ours: current });
        }
    }
    const codes = {};
    for (const d of ref) codes[d.code] = (codes[d.code] ?? 0) + 1;
    return {
        ref_count: ref.length,
        ours_count: ours.length,
        missing,
        extra,
        text_diffs: textDiffs,
        placeholder_text_diffs: placeholder,
        mutations: mutations.length,
        files: new Set(mutations.map((m) => m.file)).size,
        effective_mutations: mutations.filter((m) => ref.some((d) => d.file === m.file)).length,
        codes,
    };
}
async function batch(out, mutations) {
    restore();
    let info,
        interrupted = false;
    const children = new Set();
    const onSignal = () => {
        interrupted = true;
        for (const child of children) {
            try {
                process.kill(-child.pid, "SIGKILL");
            } catch (error) {
                if (error.code !== "ESRCH") child.kill("SIGKILL");
            }
        }
    };
    process.once("SIGINT", onSignal);
    process.once("SIGTERM", onSignal);
    try {
        apply(mutations);
        const started = performance.now(),
            settled = await Promise.allSettled(
                [
                    [REF, "ref", ["--singleThreaded"]],
                    [OURS, "ours", []],
                ].map(async ([exe, name, flags]) => {
                    const r = await runAsync(
                        [exe, "-p", ".", "--noEmit", "--incremental", "false", "--pretty", "false", ...flags],
                        {
                            cwd: clone,
                            check: false,
                            onSpawn: (child) => {
                                children.add(child);
                                child.once("close", () => children.delete(child));
                            },
                        },
                    );
                    write(path.join(out, name + ".txt"), r.stdout + r.stderr);
                    return { code: r.code, seconds: Number(((performance.now() - started) / 1000).toFixed(1)) };
                }),
            );
        requireValue(!interrupted, "mutation batch interrupted; original files restored");
        const failed = settled.find((r) => r.status === "rejected");
        if (failed) throw failed.reason;
        const result = settled.map((r) => r.value);
        info = { rc_ref: result[0].code, rc_ours: result[1].code, t_ref: result[0].seconds, t_ours: result[1].seconds };
    } finally {
        try {
            restore();
        } finally {
            process.removeListener("SIGINT", onSignal);
            process.removeListener("SIGTERM", onSignal);
        }
    }
    const result = { ...diff(out, mutations), ...info };
    writeJSON(path.join(out, "diff.json"), result);
    return result;
}
function printDiff(r) {
    console.log(
        `ref ${r.ref_count} diags, ours ${r.ours_count}; mutations ${r.mutations} in ${r.files} files, ${r.effective_mutations} with ref diagnostics`,
    );
    console.log(
        `missing ${r.missing.reduce((s, x) => s + x.n, 0)}, extra ${r.extra.reduce((s, x) => s + x.n, 0)}, text diffs ${r.text_diffs.length} (+${r.placeholder_text_diffs} placeholder)`,
    );
    for (const kind of ["missing", "extra", "text_diffs"])
        for (const d of r[kind].slice(0, 40)) console.log(kind, JSON.stringify(d));
}
function usedFiles() {
    const used = new Set();
    if (fs.existsSync(OUT))
        for (const d of fs.readdirSync(OUT)) {
            const file = path.join(OUT, d, "mutations.json");
            if (d.startsWith("batch-") && fs.existsSync(file)) for (const m of json(file)) used.add(m.file);
        }
    return used;
}
function chooseFiles(rng, count, used, heavy = 0.45, tests = 0.15) {
    const cache = path.join(OUT, "program-files.txt");
    if (!fs.existsSync(cache)) {
        const files = run([REF, "-p", ".", "--listFilesOnly"], { cwd: clone, check: false })
            .stdout.split("\n")
            .map((f) => path.resolve(clone, f.trim()))
            .filter(
                (f) =>
                    f.startsWith(clone + path.sep) &&
                    !f.includes("/node_modules/") &&
                    f.endsWith(".ts") &&
                    !f.endsWith(".d.ts"),
            )
            .map((f) => path.relative(clone, f));
        write(cache, files.join("\n") + "\n");
    }
    const files = rng.shuffle(
            read(cache)
                .split("\n")
                .filter((f) => f && !used.has(f)),
        ),
        groups = [[], [], []],
        quotas = [
            Math.floor(count * heavy),
            Math.floor(count * tests),
            count - Math.floor(count * heavy) - Math.floor(count * tests),
        ];
    for (const file of files) {
        const test = /(?:\.(?:test|spec)\.ts$|(?:^|\/)tests\/)/.test(file),
            heavyFile =
                /orm\/entities|epositor|workflow|schema|\/router\/|route|endpoint|\/temporal\/|contract|\/activities\//i.test(
                    file,
                ) ||
                /z\.object\(|z\.enum\(|@Entity|@Property|\bem\.|EntityManager|@temporalio|proxyActivities|defineEndpoint|createEndpoint|\.route\(/.test(
                    read(safe(pristine, file)),
                );
        const index = test ? 1 : heavyFile && groups[0].length < quotas[0] ? 0 : 2;
        if (groups[index].length < quotas[index]) groups[index].push(file);
        if (groups.every((g, i) => g.length >= quotas[i])) break;
    }
    return groups.flat();
}
function makeMutations(seed, files, weights) {
    const chosen = new Map();
    let pending = files;
    for (let attempt = 0; attempt < 5 && pending.length; attempt++) {
        const texts = {},
            scratch = path.join(OUT, "synchk");
        fs.rmSync(scratch, { recursive: true, force: true });
        for (const rel of pending) {
            const src = read(safe(pristine, rel)),
                seedValue = createHash("sha256").update(`${seed}:${rel}:${attempt}`).digest().readUInt32LE();
            let e;
            try {
                e = pickMutation(rel, src, random(seedValue), weights);
            } catch {
                continue;
            }
            if (!e) continue;
            const prefix = src.slice(0, e.start),
                m = {
                    file: rel,
                    ...e,
                    orig: src.slice(e.start, e.end),
                    line: prefix.split("\n").length,
                    col: e.start - (prefix.lastIndexOf("\n") + 1) + 1,
                };
            texts[rel] = src.slice(0, e.start) + e.repl + src.slice(e.end);
            chosen.set(rel, m);
            write(safe(path.resolve(scratch), rel), texts[rel]);
        }
        if (!Object.keys(texts).length) break;
        const result = run(
            [
                REF,
                "--noCheck",
                "--noLib",
                "--noResolve",
                "--noEmit",
                "--pretty",
                "false",
                "--types",
                "",
                "--experimentalDecorators",
                "--module",
                "preserve",
                "--target",
                "esnext",
                ...Object.keys(texts),
            ],
            { cwd: scratch, check: false },
        );
        requireValue([0, 1, 2].includes(result.code), "reference syntax check failed");
        pending = [
            ...new Set(
                parseDiags(result.stdout)
                    .map((d) => d.file)
                    .filter(Boolean),
            ),
        ].sort();
        for (const f of pending) chosen.delete(f);
    }
    return files.filter((f) => chosen.has(f)).map((f, id) => ({ ...chosen.get(f), id }));
}
function parseIDs(text) {
    if (text === undefined) return null;
    const ids = new Set();
    for (const part of text.split(",")) {
        requireValue(/^\d+(?:-\d+)?$/.test(part), "invalid mutation IDs");
        const [a, b = a] = part.split("-").map(Number);
        requireValue(b >= a, "invalid ID range");
        for (let i = a; i <= b; i++) ids.add(i);
    }
    return ids;
}
main(async () => {
    const command = process.argv[2],
        specs = {
            batch: { seed: null, count: 250, heavy: 0.45, tests: 0.15, "allow-reuse": false, weights: null },
            gen: { seed: null, count: 250 },
            rerun: { only: null, out: null },
            bisect: { key: null },
            restore: {},
            summary: {},
        };
    if (!command || ["--help", "-h"].includes(command)) {
        console.log(
            "Mutation commands: batch, gen, rerun <batch>, bisect <batch> --key JSON, restore, summary. Set MUT_CLONE and MUT_PRISTINE.",
        );
        return;
    }
    requireValue(command in specs, "unknown mutation command");
    const a = args(`Mutations: ${command}`, specs[command], process.argv.slice(3));
    requireValue(process.env.MUT_CLONE && process.env.MUT_PRISTINE, "set MUT_CLONE and MUT_PRISTINE");
    clone = fs.realpathSync(process.env.MUT_CLONE);
    pristine = fs.realpathSync(process.env.MUT_PRISTINE);
    requireValue(
        clone !== pristine && !clone.startsWith(pristine + path.sep) && !pristine.startsWith(clone + path.sep),
        "clone and pristine must be disjoint directories",
    );
    if (command === "restore") {
        console.log(`restored ${restore()} files`);
        return;
    }
    if (command === "summary") {
        const rows = [];
        for (const d of fs.existsSync(OUT) ? fs.readdirSync(OUT).sort() : [])
            if (d.startsWith("batch-") && fs.existsSync(path.join(OUT, d, "diff.json"))) {
                const r = json(path.join(OUT, d, "diff.json"));
                rows.push([
                    d,
                    r.mutations,
                    r.files,
                    r.effective_mutations,
                    r.ref_count,
                    r.missing.reduce((s, x) => s + x.n, 0),
                    r.extra.reduce((s, x) => s + x.n, 0),
                    r.text_diffs.length,
                ]);
            }
        console.log(
            "| batch | sites | files | sites w/ diag | ref diags | missing | extra | text diffs |\n| --- | --- | --- | --- | --- | --- | --- | --- |",
        );
        for (const row of rows) console.log("| " + row.join(" | ") + " |");
        return;
    }
    if (command === "batch" || command === "gen") {
        requireValue(a.seed !== undefined, "--seed is required");
        const files = chooseFiles(
                random(Number(a.seed)),
                a.count,
                a["allow-reuse"] ? new Set() : usedFiles(),
                a.heavy,
                a.tests,
            ),
            weights = Object.fromEntries(
                (a.weights ?? "")
                    .split(",")
                    .filter(Boolean)
                    .map((kv) => {
                        const [k, v] = kv.split("=");
                        requireValue(Number.isFinite(Number(v)) && Number(v) >= 0, "invalid mutation weight");
                        return [k, Number(v)];
                    }),
            ),
            mutations = makeMutations(a.seed, files, weights);
        if (command === "gen") {
            for (const m of mutations) console.log(JSON.stringify(m));
            return;
        }
        const out = path.join(OUT, `batch-${String(a.seed).padStart(3, "0")}`);
        requireValue(!fs.existsSync(path.join(out, "mutations.json")), "batch already exists; use rerun");
        writeJSON(path.join(out, "mutations.json"), mutations);
        printDiff(await batch(out, mutations));
        return;
    }
    requireValue(a._.length === 1, "one batch directory is required");
    const dir = a._[0],
        all = json(path.join(dir, "mutations.json"));
    if (command === "rerun") {
        const ids = parseIDs(a.only),
            mutations = ids ? all.filter((m) => ids.has(m.id)) : all;
        printDiff(await batch(a.out ?? path.join(dir, "rerun"), mutations));
        return;
    }
    requireValue(a.key, "--key is required");
    const key = JSON.parse(a.key);
    let n = 0,
        candidates = all;
    const test = async (subset) => {
        const r = await batch(path.join(dir, `bisect-${++n}`), subset);
        return [...r.missing, ...r.extra].some((x) => JSON.stringify(x.key) === JSON.stringify(key));
    };
    const sameFile = all.filter((m) => m.file === key[0]);
    if (sameFile.length && (await test(sameFile))) candidates = sameFile;
    while (candidates.length > 1) {
        const half = Math.floor(candidates.length / 2),
            left = candidates.slice(0, half),
            right = candidates.slice(half);
        if (await test(left)) candidates = left;
        else if (await test(right)) candidates = right;
        else {
            let changed = false;
            for (let i = 0; i < candidates.length; i++) {
                const trial = candidates.filter((_, j) => i !== j);
                if (await test(trial)) {
                    candidates = trial;
                    changed = true;
                    break;
                }
            }
            if (!changed) break;
        }
    }
    console.log("minimal:", JSON.stringify(candidates, null, 2));
});
