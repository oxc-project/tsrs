#!/usr/bin/env node
import path from "node:path";
import { ROOT, args, required, requireValue, runAsync, write, writeJSON, main } from "../lib/cli.mjs";
import { projects, command, timed, publicRun, median, fmt, pct, table, machineInfo } from "./common.mjs";
const EVENTS = "instructions,cycles,iTLB-load-misses,L1-icache-load-misses";
main(async () => {
    const a = args("Interleaved measurements of several tsrs builds; first --bin is the baseline.", {
        bin: [],
        projects: "all",
        checkers: "default,4",
        "identity-checkers": "1,4,32",
        reps: 7,
        perf: null,
        "work-dir": path.join(ROOT, "bench/.work"),
        out: "probe-out",
    });
    requireValue(
        a.bin.length > 1 && Number.isInteger(a.reps) && a.reps > 0,
        "provide at least two --bin NAME=PATH options and positive --reps",
    );
    const bins = Object.fromEntries(
            a.bin.map((b) => {
                const i = b.indexOf("=");
                requireValue(i > 0, "--bin must be NAME=PATH");
                return [b.slice(0, i), path.resolve(b.slice(i + 1))];
            }),
        ),
        names = Object.keys(bins);
    requireValue(
        names.length === a.bin.length && names.every((n) => /^[\w.-]+$/.test(n)),
        "binary names must be unique safe filenames",
    );
    const ps = projects(a.projects, a["work-dir"]),
        ks = a.checkers.split(","),
        out = path.resolve(a.out),
        result = { bins, runs: [], identity: [], perf: [], machine: machineInfo() };
    requireValue(
        ks.every((k) => k === "default" || /^[1-9]\d*$/.test(k)),
        "invalid checker count",
    );
    const once = async (n, p, k, tag) => {
        const r = await timed(command(bins[n], p, k, false), { cwd: p.cwd, log: path.join(out, "logs", tag + ".txt") });
        requireValue(r.ok, `compiler failed; see ${r.log}`);
        return r;
    };
    for (const n of names) for (const p of ps) await once(n, p, ks[0], `warm-${n}-${p.name}`);
    for (let rep = 0; rep < a.reps; rep++) {
        for (const p of ps)
            for (const k of ks)
                for (const n of [...names.slice(rep % names.length), ...names.slice(0, rep % names.length)])
                    result.runs.push({
                        bin: n,
                        project: p.name,
                        checkers: k,
                        rep,
                        ...publicRun(await once(n, p, k, `${n}-${p.name}-${k}-${rep}`)),
                    });
        console.error(`round ${rep + 1}/${a.reps} done`);
    }
    for (const p of ps)
        for (const k of a["identity-checkers"].split(",")) {
            const outputs = {};
            for (const n of names) {
                const r = await runAsync(command(bins[n], p, k, false), { cwd: p.cwd, check: false });
                outputs[n] = `${r.stdout}${r.stderr}\nexit ${r.code}\n`;
                write(path.join(out, "logs", `out-${n}-${p.name}-${k}.txt`), outputs[n]);
            }
            for (const n of names.slice(1))
                result.identity.push({
                    project: p.name,
                    checkers: k,
                    bin: n,
                    identical: outputs[n] === outputs[names[0]],
                    bytes: Buffer.byteLength(outputs[n]),
                });
        }
    if (a.perf)
        for (const p of ps)
            for (const n of names) {
                const file = path.join(out, "logs", `perf-${n}-${p.name}.csv`);
                const r = await runAsync(
                    [a.perf, "stat", "-x", ",", "-o", file, "-e", EVENTS, "--", ...command(bins[n], p, 1)],
                    { cwd: p.cwd, check: false },
                );
                const fs = await import("node:fs");
                const text = fs.existsSync(file) ? fs.readFileSync(file, "utf8") : r.stderr;
                const counts = {};
                for (const line of text.split("\n")) {
                    const parts = line.split(",");
                    if (/^\d+(\.\d+)?$/.test(parts[0]) && parts[2]) counts[parts[2].split(":")[0]] = Number(parts[0]);
                }
                result.perf.push({ bin: n, project: p.name, ...counts });
            }
    const rows = [];
    for (const p of ps)
        for (const k of ks) {
            const cells = Object.fromEntries(
                names.map((n) => [
                    n,
                    result.runs.filter((r) => r.bin === n && r.project === p.name && r.checkers === k),
                ]),
            );
            for (const n of names)
                rows.push([
                    p.name,
                    k,
                    n,
                    fmt(median(cells[n].map((r) => r.wall_s))),
                    pct(median(cells[names[0]].map((r) => r.wall_s)), median(cells[n].map((r) => r.wall_s))),
                    fmt(median(cells[n].map((r) => r.rss_b)) / 2 ** 30),
                    pct(median(cells[names[0]].map((r) => r.rss_b)), median(cells[n].map((r) => r.rss_b))),
                ]);
        }
    const same = result.identity.every((r) => r.identical),
        md =
            table(["project", "checkers", "binary", "wall s", "Δ wall", "peak GiB", "Δ RSS"], rows) +
            "\n\n" +
            (result.perf.length
                ? table(
                      ["project", "binary", ...EVENTS.split(",")],
                      result.perf.map((r) => [r.project, r.bin, ...EVENTS.split(",").map((e) => r[e] ?? "n/a")]),
                  ) + "\n\n"
                : "") +
            `Output identity: ${same ? "yes" : "NO"} (${result.identity.length} comparisons).\n`;
    writeJSON(path.join(out, "layout.json"), result);
    write(path.join(out, "layout.md"), md);
    console.log(md);
    process.exitCode = same ? 0 : 1;
});
