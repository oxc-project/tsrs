#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { ROOT, args, read, requireValue, main } from "./lib/cli.mjs";
const keep = new Set(
    "typeof import readonly keyof unique symbol infer extends new abstract asserts is any unknown never undefined null void this true false".split(
        " ",
    ),
);
const skeleton = (s) =>
    s
        .replace(/\b\d+(\.\d+)?\b/g, "0")
        .replace(/[A-Za-z_$#][\w$]*/g, (m) => (keep.has(m) ? m : "_"))
        .replace(/\s+/g, " ");
function normalize(line, types) {
    if (!line) return "<none>";
    let s = line.replace(/^[+-]/, "");
    if (types)
        return s.startsWith(">") && s.includes(" : ")
            ? "> : " + skeleton(s.slice(s.indexOf(" : ") + 3).replace(/Decl\([^)]*\)/g, "Decl(F)"))
            : skeleton(s.trim());
    s = s
        .trim()
        .replace(/^[^\s(]+\(\d+,\d+\)/, "FILE(L,C)")
        .replace(/\S+\.\w+:\d+:\d+/g, "FILE:L:C")
        .replace(/'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)"/g, (_, a, b) => `'${skeleton(a ?? b)}'`);
    return /^[~ ]*$/.test(s) ? (s.trim() ? "~" : "") + (s.trim().length > 1 ? "..." : "") : s;
}
main(() => {
    const a = args("Cluster failures by their first differing line pair.", {
        class: null,
        baseline: "errors",
        results: path.join(ROOT, "target/test-results"),
        top: 40,
        examples: 3,
    });
    requireValue(["errors", "types", "symbols"].includes(a.baseline), "invalid baseline");
    const extra = a.baseline !== "errors",
        klass = a.class ?? (extra ? "fail" : "codes"),
        name = extra ? `${a.baseline}-${klass}` : klass;
    const ids = read(path.join(a.results, `${name}.txt`))
            .split("\n")
            .map((s) => s.trim())
            .filter(Boolean),
        groups = new Map();
    for (const id of ids) {
        const file = path.join(a.results, id + (extra ? `.${a.baseline}.diff` : ".diff"));
        let pair;
        if (!fs.existsSync(file)) pair = ["<no diff file>", ""];
        else {
            let minus,
                plus,
                hunk = false;
            for (const line of read(file).split("\n")) {
                if (line.startsWith("@@")) {
                    if (hunk && (minus || plus)) break;
                    hunk = true;
                    continue;
                }
                if (!hunk || /^(---|\+\+\+)/.test(line)) continue;
                if (line.startsWith("-") && minus === undefined) minus = line;
                else if (line.startsWith("+") && plus === undefined) plus = line;
                if (minus !== undefined && plus !== undefined) break;
            }
            pair = [normalize(minus, extra), normalize(plus, extra)];
        }
        const key = JSON.stringify(pair);
        if (!groups.has(key)) groups.set(key, []);
        groups.get(key).push(id);
    }
    console.log(`${ids.length} tests in class '${name}', ${groups.size} clusters\n`);
    for (const [key, tests] of [...groups]
        .sort((a, b) => b[1].length - a[1].length || (a[0] < b[0] ? -1 : 1))
        .slice(0, a.top)) {
        const [exp, act] = JSON.parse(key);
        console.log(
            `${String(tests.length).padStart(5)}  - ${exp}\n       + ${act}\n       e.g. ${tests.slice(0, a.examples).join(", ")}\n`,
        );
    }
});
