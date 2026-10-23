#!/usr/bin/env node
import { performance } from "node:perf_hooks";
import path from "node:path";
import { main, run, requireValue } from "../lib/cli.mjs";
import { WatchSession, positional, STRINGS, LIFECYCLE } from "./session.mjs";
main(async () => {
    const a = positional("Usage: watchlat <compiler> <project> [compiler args...]");
    if (!a) return;
    const s = new WatchSession(a[0], a[1], a.slice(2)),
        repeat = Number(process.env.REPEAT ?? 1);
    requireValue(Number.isInteger(repeat) && repeat > 0, "REPEAT must be positive");
    const rss = () => {
        const r = run(["ps", "-o", "rss=", "-p", String(s.proc.pid)], { check: false });
        return r.stdout.trim() ? Math.floor(Number(r.stdout.trim()) / 1024) : -1;
    };
    const footprint = () => {
        if (process.platform !== "darwin") return "";
        const r = run(["footprint", "-p", String(s.proc.pid)], { check: false });
        const m = r.stdout.match(/Footprint:\s*([\d.]+)\s*(GB|MB|KB|B)\b/);
        return m
            ? ` footprint ${Math.round(Number(m[1]) * { GB: 1024, MB: 1, KB: 1 / 1024, B: 1 / 1048576 }[m[2]])} MB`
            : "";
    };
    const print = (name, t, cycles) => {
        const last = cycles.at(-1),
            det = last.detected === null ? NaN : last.detected - t;
        console.log(
            `${path.basename(a[0])} ${name}: detected ${det.toFixed(1)} ms done ${(last.done - t).toFixed(0)} ms cycles ${cycles.length} rss ${rss()} MB${footprint()} ${last.summary}`,
        );
    };
    const edits = [
        ["body comment, leaf-ish", STRINGS, "\n// watchbench edit\n"],
        ["body comment again", STRINGS, "\n// watchbench edit 2\n"],
        ["new export, hub file", LIFECYCLE, "\nexport const watchbenchX = 1;\n"],
        ["type error in leaf", STRINGS, "\nconst watchbenchErr: number = 'x';\n"],
    ];
    try {
        const start = performance.now();
        print("initial", start, await s.cycles());
        for (let i = 0; i < repeat; i++)
            for (const [name, rel, text] of edits) {
                const t = performance.now();
                s.edit(rel, (x) => x + text);
                print(name, t, await s.cycles());
            }
    } finally {
        await s.close();
    }
});
