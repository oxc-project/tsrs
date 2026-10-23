#!/usr/bin/env node
import path from "node:path";
import { args, required, write, writeJSON, runAsync, main, requireValue } from "../lib/cli.mjs";
import {
    projects,
    command,
    timed,
    publicRun,
    envSpec,
    baseEnv,
    positiveList,
    report,
    hash,
    median,
    fmt,
    table,
} from "./common.mjs";
main(async () => {
    const a = args("Compare checker-assignment variants with separate untimed phase statistics.", {
        work: null,
        out: null,
        projects: "vscode",
        checkers: "16,32",
        reps: 10,
        "stats-reps": 2,
        bin: [],
        variant: [],
    });
    required(a, "work", "out");
    requireValue(
        Number.isInteger(a.reps) &&
            a.reps > 0 &&
            Number.isInteger(a["stats-reps"]) &&
            a["stats-reps"] >= 0 &&
            a.variant.length,
        "positive --reps and at least one --variant are required",
    );
    const bins = Object.fromEntries(
        a.bin.map((b) => {
            const i = b.indexOf("=");
            requireValue(i > 0, "--bin must be key=path");
            return [b.slice(0, i), path.resolve(b.slice(i + 1))];
        }),
    );
    const vs = a.variant.map((s) => {
        const [name, key, ...env] = s.split(":");
        requireValue(bins[key] && /^[\w.-]+$/.test(name), "variant must name a --bin key");
        return { name, bin: bins[key], env: envSpec(env.join(":")) };
    });
    requireValue(new Set(vs.map((v) => v.name)).size === vs.length, "duplicate variants");
    const out = path.resolve(a.out),
        results = [],
        env = baseEnv();
    for (const p of projects(a.projects, a.work))
        for (const k of positiveList(a.checkers)) {
            await runAsync(command(vs[0].bin, p, k, false), { cwd: p.cwd, env, check: false });
            for (let rep = 0; rep < a.reps + a["stats-reps"]; rep++)
                for (const v of vs) {
                    const stats = rep >= a.reps,
                        cmd = [...command(v.bin, p, k, false), ...(stats ? ["--extendedDiagnostics"] : [])],
                        log = path.join(out, "runs", `${p.name}-c${k}-${v.name}-${stats ? "stats" : "rep"}${rep}.txt`);
                    let r;
                    if (stats) {
                        const x = await runAsync(cmd, { cwd: p.cwd, env: { ...env, ...v.env }, check: false }),
                            text = x.stdout + "\n" + x.stderr;
                        write(log, text);
                        r = {
                            exit: x.code,
                            diagnostics: hash(text),
                            ok: [0, 1, 2].includes(x.code) && !/panicked at|^panic:|^fatal error:/m.test(text),
                        };
                        for (const phase of ["Check time", "Total time"]) {
                            const m = text.match(new RegExp(`^${phase}:\\s+([\\d.]+)s`, "m"));
                            if (m) r[phase] = Number(m[1]);
                        }
                    } else r = publicRun(await timed(cmd, { cwd: p.cwd, env: { ...env, ...v.env }, log }));
                    results.push({ project: p.name, checkers: k, variant: v.name, rep, stats, ...r });
                    console.error(`${p.name} c${k} ${v.name} ${stats ? "stats" : "rep"}${rep}: exit ${r.exit}`);
                }
        }
    let identical = true;
    const rows = [];
    for (const p of new Set(results.map((r) => r.project)))
        for (const k of new Set(results.filter((r) => r.project === p).map((r) => r.checkers))) {
            const rs = results.filter((r) => r.project === p && r.checkers === k);
            identical &&= rs.every((r) => r.ok) && new Set(rs.map((r) => `${r.exit}:${r.diagnostics}`)).size === 1;
            for (const v of vs) {
                const stats = rs.filter((r) => r.stats && r.variant === v.name);
                rows.push([
                    p,
                    k,
                    v.name,
                    fmt(median(stats.map((r) => r["Check time"]))),
                    fmt(median(stats.map((r) => r["Total time"]))),
                ]);
            }
        }
    writeJSON(path.join(out, "results.json"), results);
    const md =
        report(
            results,
            vs.map((v) => v.name),
        ) +
        "\n" +
        table(["project", "checkers", "variant", "check s (untimed)", "total s (untimed)"], rows) +
        `\n\nDiagnostics and exits identical in every cell: ${identical ? "yes" : "NO"}.\n`;
    write(path.join(out, "summary.md"), md);
    console.log(md);
    process.exitCode = identical ? 0 : 1;
});
