#!/usr/bin/env node
// Interleave baseline/candidate runs and require identical ordered diagnostics and exit codes.
import path from "node:path";
import { ROOT, args, required, json, write, writeJSON, run, main, isMain, requireValue } from "../lib/cli.mjs";
import { projects, command, timed, publicRun, diagnostics, median, pct, fmt, table, machineInfo } from "./common.mjs";
import { measure } from "../../bench/measure.mjs";
export function firstDifference(a, b) {
    for (let i = 0; i < Math.max(a.length, b.length); i++)
        if (a[i] !== b[i]) return { line: i + 1, base: a[i] ?? "<end of output>", new: b[i] ?? "<end of output>" };
    return null;
}
export function compareCell(base, next) {
    const ref = base[0],
        problems = [];
    for (const [which, rs] of [
        ["base", base],
        ["new", next],
    ])
        rs.forEach((r, i) => {
            if (!r.ok) problems.push(`${which} rep ${i}: did not finish (exit ${r.exit})`);
            if (r.exit !== ref.exit) problems.push(`${which} rep ${i}: exit ${r.exit}, base rep 0 exit ${ref.exit}`);
            const d = firstDifference(ref._diagnostics, r._diagnostics);
            if (d)
                problems.push(`${which} rep ${i}: diagnostics differ at line ${d.line}: base ${d.base}, new ${d.new}`);
        });
    return {
        identical: !problems.length,
        problems,
        errors: { base: ref.errors, new: next[0].errors },
        full_text_first_difference: firstDifference(ref._text, next[0]._text),
    };
}
const summarize = (rs) =>
    Object.fromEntries(
        ["wall_s", "user_s", "peak_rss_bytes"].map((k) => [k, median(rs.filter((r) => r.ok).map((r) => r[k]))]),
    );
export function markdown(result) {
    const cells = Object.entries(result.projects).flatMap(([name, p]) => p.cells.map((c) => [name, c]));
    return (
        `Identical cells: ${cells.filter(([, c]) => c.identical).length}/${cells.length}. Base ${result.base.label}; new ${result.new.label}.\n\n` +
        table(
            ["project", "mode", "identical", "base wall s", "new wall s", "Δ wall", "Δ RSS", "Δ instructions"],
            cells.map(([p, c]) => [
                p,
                c.label,
                c.identical ? "yes" : "**NO**",
                fmt(c.base?.wall_s),
                fmt(c.new?.wall_s),
                pct(c.base?.wall_s, c.new?.wall_s),
                pct(c.base?.peak_rss_bytes, c.new?.peak_rss_bytes),
                pct(c.base?.instructions, c.new?.instructions),
            ]),
        ) +
        "\n\n" +
        cells
            .filter(([, c]) => !c.identical)
            .map(([p, c]) => `${p} ${c.label}: ${c.problems.join("; ")}`)
            .join("\n") +
        `\n\nMachine: ${result.machine.label}. Duration: ${result.duration_s}s. Flags: ${result.flags}.\n`
    );
}
if (isMain(import.meta.url))
    main(async () => {
        const a = args("Verify a candidate tsrs against a baseline on benchmark projects.", {
            base: null,
            new: null,
            "base-label": null,
            "new-label": null,
            "base-commit": "",
            "new-commit": "",
            "new-dirty": false,
            projects: "all",
            checkers: "1,4,16,32",
            reps: 3,
            poison: true,
            timeout: 900,
            "work-dir": path.join(ROOT, "bench/.work"),
            out: "verify-out",
            render: false,
        });
        const out = path.resolve(a.out);
        if (a.render) {
            write(path.join(out, "verify.md"), markdown(json(path.join(out, "verify.json"))));
            return;
        }
        required(a, "base", "new");
        requireValue(Number.isInteger(a.reps) && a.reps > 0, "--reps must be positive");
        const checkers = a.checkers.split(",").map(Number);
        requireValue(
            checkers.every((n) => Number.isInteger(n) && n > 0),
            "checker counts must be positive",
        );
        const bins = { base: path.resolve(a.base), new: path.resolve(a.new) };
        const result = {
            date: new Date().toISOString(),
            machine: machineInfo(),
            projects_selected: [],
            checkers,
            reps: a.reps,
            poison: a.poison,
            flags: "--noEmit --incremental false --pretty false",
            projects: {},
            runs: [],
        };
        for (const w of ["base", "new"]) {
            const version = run([bins[w], "--version"]).stdout.trim();
            result[w] = {
                binary: bins[w],
                version,
                commit: a[`${w}-commit`],
                label: a[`${w}-label`] ?? version,
                ...(w === "new" ? { dirty: a["new-dirty"] } : {}),
            };
        }
        const start = performance.now();
        for (const p of projects(a.projects, a["work-dir"])) {
            run(
                [
                    "node",
                    path.join(ROOT, "bench/bench.mjs"),
                    "prepare",
                    "--projects",
                    p.name,
                    "--work-dir",
                    path.resolve(a["work-dir"]),
                ],
                { inherit: true, timeout: a.timeout },
            );
            result.projects_selected.push(p.name);
            const once = async (w, k, label, env) =>
                timed(command(bins[w], p, k, false), {
                    cwd: p.cwd,
                    env,
                    timeout: a.timeout,
                    log: path.join(out, "logs", p.name, label + ".out"),
                });
            await once("base", Math.max(...checkers), "warmup");
            const runs = Object.fromEntries(checkers.map((k) => [k, { base: [], new: [] }]));
            for (let rep = 0; rep < a.reps; rep++)
                for (const k of checkers)
                    for (const w of rep % 2 ? ["new", "base"] : ["base", "new"]) {
                        const r = await once(w, k, `checkers${k}-${w}-${rep}`);
                        runs[k][w].push(r);
                        result.runs.push({ project: p.name, checkers: k, binary: w, rep, ...publicRun(r) });
                        console.error(`${p.name} c${k} ${w} rep${rep}: ${fmt(r.wall)}s, exit ${r.exit}`);
                    }
            const cells = checkers.map((k) => ({
                kind: "checkers",
                checkers: k,
                label: `${k} checker${k === 1 ? "" : "s"}`,
                ...compareCell(runs[k].base, runs[k].new),
                base: summarize(runs[k].base),
                new: summarize(runs[k].new),
            }));
            const counts = {};
            for (const w of ["base", "new"]) {
                const r = measure(command(bins[w], p, 1), {
                        cwd: p.cwd,
                        env: { ...process.env, RAYON_NUM_THREADS: "1" },
                        timeout: a.timeout,
                        instructions: true,
                    }),
                    text = r.stdout + "\n" + r.stderr;
                write(path.join(out, "logs", p.name, `instructions-${w}.log`), text);
                counts[w] = {
                    exit: r.code,
                    instructions: r.instructions,
                    peak_rss_bytes: r.max_rss_bytes,
                    count_error: r.error ?? null,
                    _diagnostics: diagnostics(text),
                    _text: text.split("\n"),
                    errors: diagnostics(text).filter((l) => /error TS\d+:/.test(l)).length,
                    ok: [0, 1, 2].includes(r.code) && !/^panic:|^fatal error:|panicked at/m.test(text),
                };
            }
            cells.push({
                kind: "instructions",
                label: "single-threaded",
                ...compareCell([counts.base], [counts.new]),
                base: publicRun(counts.base),
                new: publicRun(counts.new),
            });
            if (a.poison) {
                const ref = runs[16]?.base[0] ?? (await once("base", 16, "poison-reference-base")),
                    r = await once("new", 16, "poison-new", { ...process.env, TSRS_ARENA_POISON: "1" });
                cells.push({
                    kind: "poison",
                    checkers: 16,
                    label: "poisoned arenas, 16 checkers",
                    ...compareCell([ref], [r]),
                    new: publicRun(r),
                });
            }
            for (const c of cells)
                for (const [field, key] of [
                    ["wall", "wall_s"],
                    ["rss", "peak_rss_bytes"],
                    ["user", "user_s"],
                    ["instructions", "instructions"],
                ])
                    c[`${field}_delta_pct`] =
                        c.base?.[key] && c.new?.[key] != null ? (c.new[key] / c.base[key] - 1) * 100 : null;
            result.projects[p.name] = { cells };
        }
        result.duration_s = Math.round((performance.now() - start) / 1000);
        writeJSON(path.join(out, "verify.json"), result);
        const md = markdown(result);
        write(path.join(out, "verify.md"), md);
        console.log(md);
        process.exitCode = Object.values(result.projects).some((p) => p.cells.some((c) => !c.identical)) ? 1 : 0;
    });
