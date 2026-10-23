#!/usr/bin/env node
import { args, read, median, main } from "../../tools/lib/cli.mjs";
main(() => {
    const a = args("Render medians from measure.mjs JSONL files. <file>..."),
        groups = new Map();
    for (const file of a._)
        for (const line of read(file).split("\n").filter(Boolean)) {
            const r = JSON.parse(line),
                key = JSON.stringify([r.mode, r.label]);
            if (!groups.has(key)) groups.set(key, []);
            groups.get(key).push(r);
        }
    console.log(
        "| mode | build | n | symbols | types | instantiations | allocs (M) | check s | Go mem used GB | real s | peak footprint GB | max RSS GB | load |\n| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    );
    for (const [key, rows] of [...groups].sort(
        (a, b) => Number(JSON.parse(a[0])[0] !== "single") - Number(JSON.parse(b[0])[0] !== "single"),
    )) {
        const [mode, label] = JSON.parse(key),
            fmt = (k, scale = 1, digits = 2) => {
                const n = median(rows.map((r) => r[k]));
                return n === null
                    ? "-"
                    : (n * scale).toLocaleString("en-US", {
                          minimumFractionDigits: digits,
                          maximumFractionDigits: digits,
                      });
            };
        console.log(
            "| " +
                [
                    mode,
                    label,
                    rows.length,
                    fmt("Symbols", 1, 0) + (new Set(rows.map((r) => r.Symbols)).size > 1 ? " (varies)" : ""),
                    fmt("Types", 1, 0),
                    fmt("Instantiations", 1, 0),
                    fmt("Memory allocs", 1e-6),
                    fmt("Check time"),
                    fmt("Memory used", 1024 / 1e9),
                    fmt("real"),
                    fmt("peak", 1e-9),
                    fmt("maxrss", 1e-9),
                    fmt("load", 1, 0),
                ].join(" | ") +
                " |",
        );
    }
});
