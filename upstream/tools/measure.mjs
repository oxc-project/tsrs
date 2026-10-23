#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { args, requireValue, run, main } from "../../tools/lib/cli.mjs";
import { measure } from "../../bench/measure.mjs";
main(() => {
    const a = args(
        "Measure interleaved compiler runs. <out.jsonl> <rounds> <single,multi> <label=binary[:K=V;K=V]>...",
    );
    requireValue(a._.length >= 4, "output, rounds, modes and compiler specs are required");
    const [out, rounds, modes, ...specs] = a._,
        project = process.env.PROJECT;
    requireValue(project, "set PROJECT to the tsconfig directory to measure");
    const stats = process.env.STATS === "1",
        cwd = process.env.MEASURE_CWD ?? path.dirname(new URL(import.meta.url).pathname);
    for (let round = 0; round < Number(rounds); round++)
        for (const mode of modes.split(","))
            for (const spec of specs) {
                const i = spec.indexOf("="),
                    label = spec.slice(0, i),
                    rest = spec.slice(i + 1),
                    colon = rest.indexOf(":"),
                    binary = colon < 0 ? rest : rest.slice(0, colon),
                    env = Object.fromEntries(
                        (colon < 0 ? "" : rest.slice(colon + 1))
                            .split(";")
                            .filter(Boolean)
                            .map((kv) => {
                                const j = kv.indexOf("=");
                                return [kv.slice(0, j), kv.slice(j + 1)];
                            }),
                    ),
                    command = [
                        binary,
                        "-p",
                        project,
                        "--noEmit",
                        "--incremental",
                        "false",
                        ...(stats ? ["--extendedDiagnostics"] : []),
                        ...(mode === "single"
                            ? ["--singleThreaded"]
                            : [
                                  "--checkers",
                                  "4",
                                  ...(path.basename(binary) === "tsrs" ? ["--checkerAssignment", "go"] : []),
                              ]),
                    ],
                    p = stats
                        ? run(command, { cwd, env: { ...process.env, ...env }, check: false })
                        : measure(command, { cwd, env: { ...process.env, ...env } }),
                    row = {
                        label,
                        mode,
                        round,
                        exit: p.code,
                        load: os.loadavg()[0],
                        env,
                        errors_lines: [...p.stdout.matchAll(/error TS\d+/g)].length,
                    };
                for (const key of [
                    "Symbols",
                    "Types",
                    "Instantiations",
                    "Memory used",
                    "Memory allocs",
                    "Check time",
                    "Total time",
                    "Errors",
                ]) {
                    const m = p.stdout.match(new RegExp(`^${key}:\\s+([\\d.]+)`, "m"));
                    if (m) row[key] = Number(m[1]);
                }
                if (!stats)
                    Object.assign(row, { real: p.wall_s, maxrss: p.max_rss_bytes, peak: p.peak_footprint_bytes });
                fs.appendFileSync(out, JSON.stringify(row) + "\n");
                console.log(
                    `${label} ${mode} r${round} sym=${row.Symbols} types=${row.Types} inst=${row.Instantiations} real=${row.real} exit=${p.code}`,
                );
            }
});
