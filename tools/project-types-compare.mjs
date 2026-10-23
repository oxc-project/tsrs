#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { args, read, write, requireValue, main } from "./lib/cli.mjs";
import { unified } from "./lib/diff.mjs";
function load(file) {
    const entries = new Map();
    let counts;
    for (const line of read(file).trimEnd().split("\n")) {
        const [hash, n, rel, ...rest] = line.split("\t");
        if (hash === "#counts") counts = [n, rel, ...rest];
        else entries.set(rel, [hash, Number(n)]);
    }
    return { entries, counts };
}
main(() => {
    const a = args("Compare project .types or .symbols dumps. <go-out> <rs-out>", {
        kind: "types",
        list: null,
        show: 0,
        subset: false,
    });
    requireValue(a._.length === 2, "two dump directories are required");
    const [go, rs] = a._,
        g = load(path.join(go, `manifest.${a.kind}`)),
        r = load(path.join(rs, `manifest.${a.kind}`));
    if (a.subset) for (const p of g.entries.keys()) if (!r.entries.has(p)) g.entries.delete(p);
    const onlyG = [...g.entries.keys()].filter((p) => !r.entries.has(p)),
        onlyR = [...r.entries.keys()].filter((p) => !g.entries.has(p)),
        diff = [...g.entries.keys()].filter((p) => r.entries.has(p) && g.entries.get(p)[0] !== r.entries.get(p)[0]);
    console.log(
        `files: ${g.entries.size - onlyG.length - diff.length}/${g.entries.size} identical (${diff.length} differ, ${onlyG.length} only in go, ${onlyR.length} only in rs)`,
    );
    console.log(`lines (go): ${[...g.entries.values()].reduce((s, [, n]) => s + n, 0)}`);
    console.log(`counts go: ${JSON.stringify(g.counts)}  rs: ${JSON.stringify(r.counts)}`);
    for (const p of onlyG.slice(0, 10)) console.log("only go:", p);
    for (const p of onlyR.slice(0, 10)) console.log("only rs:", p);
    if (a.list) write(a.list, diff.map((p) => p + "\n").join(""));
    let total = 0,
        missing = 0,
        shown = 0;
    for (const p of diff) {
        const gp = path.join(go, a.kind, `${p}.${a.kind}`),
            rp = path.join(rs, a.kind, `${p}.${a.kind}`);
        if (!fs.existsSync(gp) || !fs.existsSync(rp)) {
            missing++;
            continue;
        }
        const patch = unified(read(gp), read(rp), p, p);
        let removed = 0,
            added = 0;
        const flush = () => {
            total += Math.max(removed, added);
            removed = added = 0;
        };
        for (const line of patch.split("\n")) {
            if (line.startsWith("---") || line.startsWith("+++")) continue;
            if (line.startsWith("-")) removed++;
            else if (line.startsWith("+")) added++;
            else if (!line.startsWith("\\")) flush();
        }
        flush();
        if (shown++ < a.show) console.log(patch);
    }
    if (diff.length) console.log(`differing lines: ${total} (text missing for ${missing} files)`);
});
