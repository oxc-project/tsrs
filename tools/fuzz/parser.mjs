#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { ROOT, args, read, write, walk, random, runAsync, requireValue, main } from "../lib/cli.mjs";
import { hashAll } from "../oracle/hash.mjs";
import { unified } from "../lib/diff.mjs";
const TOKENS = [
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    "<",
    ">",
    "=>",
    "?.",
    "??",
    "?",
    ":",
    ";",
    ",",
    ".",
    "...",
    "@",
    "#",
    "!",
    "=",
    "`",
    "${",
    "/",
    "/*",
    "*/",
    "//",
    "'",
    '"',
    "\\",
    "\n",
    "\r\n",
    "\u2028",
    "\uFEFF",
    "\u200B",
    "\\uD800",
    "<div>",
    "</div>",
    "<>",
    "</>",
    "{...x}",
    "<T,>",
    "<T extends U>",
    "as const",
    "satisfies",
    "readonly",
    "async",
    "await",
    "yield",
    "function*",
    "class",
    "enum",
    "namespace",
    "declare",
    "abstract",
    "accessor",
    "import(",
    "import.meta",
    "export =",
    "export default",
    "type",
    "interface",
    "keyof",
    "infer",
    "asserts",
    "unique symbol",
    "this",
    "super",
    "new.target",
    "0x",
    "0b1_",
    "1n",
    ".5e+",
    "1__0",
    "08",
    "/[a-z]/gu",
    "/(?<n>.)\\k<n>/",
    "\\u{10FFFF}",
    "#!",
    "@ts-ignore",
    "/** @type {X} */",
    '/// <reference path="x" />',
    "using",
    "await using",
    "out",
    "in",
    "const enum",
    "get",
    "set",
    "static {",
    "#priv",
    "override",
];
function mutate(rng, source, otherSource) {
    let text = [...source],
        other = [...otherSource];
    for (let k = rng.int(1, 3); k > 0; k--) {
        const n = text.length,
            kind = rng.int(0, 6),
            bounds = [rng.int(0, n), rng.int(0, n)].sort((a, b) => a - b),
            i = bounds[0],
            j = Math.min(bounds[1], i + rng.int(1, 200));
        if (kind === 0 && n) text.splice(i, j - i);
        else if (kind === 1 && n) text.splice(j, 0, ...text.slice(i, j));
        else if (kind === 2) text.splice(i, 0, ...rng.choice(TOKENS));
        else if (kind === 3 && n) {
            const [a, b] = [i, rng.int(0, n)].sort((a, b) => a - b),
                span = j - i;
            text = [
                ...text.slice(0, a),
                ...text.slice(b, b + span),
                ...text.slice(a + span, b),
                ...text.slice(a, a + span),
                ...text.slice(b + span),
            ];
        } else if (kind === 4) {
            const a = rng.int(0, other.length);
            text.splice(i, 0, ...other.slice(a, a + rng.int(1, 400)));
        } else if (kind === 5 && n) text = text.slice(0, i);
        else if (n) text.splice(i, 1, [...rng.choice(TOKENS)][0]);
    }
    return text.join("");
}
main(async () => {
    const a = args("Differential parser fuzzing against the pinned Go oracle.", {
            rounds: 5,
            "per-round": 2000,
            seed: Math.floor(Date.now() / 1000),
            jobs: Math.max(1, Math.floor(os.availableParallelism() / 2)),
            out: path.join(ROOT, "target/fuzz-parser"),
        }),
        go = process.env.ORACLE_GO ?? path.resolve(ROOT, "../bin/tsrs-oracle-ast"),
        rs = process.env.ORACLE_RS ?? path.join(ROOT, "target/parser-integrate/release/examples/ast_oracle");
    for (const b of [go, rs]) requireValue(fs.existsSync(b), `missing ${b}; build with sh tools/oracle/ast/build.sh`);
    const rng = random(a.seed),
        files = [
            ...walk(path.join(ROOT, "ts-ref/tsc/testdata/tests/cases"), (f) => /\.(ts|tsx|mts|cts|js|jsx)$/.test(f)),
        ].sort();
    requireValue(files.length, "no corpus");
    let findings = 0,
        total = 0,
        compared = 0;
    fs.mkdirSync(a.out, { recursive: true });
    for (let round = 0; round < a.rounds; round++) {
        const work = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-fuzz-"));
        try {
            const lines = [];
            for (let k = 0; k < a["per-round"]; k++) {
                const src = rng.choice(files),
                    file = path.join(work, `m${round}_${k}${path.extname(src)}`);
                write(file, mutate(rng, read(src), read(rng.choice(files))));
                lines.push(file + "\t" + rng.choice(["-", "j", "f", "jf"]));
            }
            const before = await hashAll(go, lines, a.jobs),
                after = await hashAll(rs, lines, a.jobs);
            for (const line of lines) {
                const [file, flags] = line.split("\t");
                if (!before[file] || before[file] === "CRASH") continue;
                compared++;
                if (before[file] === after[file]) continue;
                findings++;
                const kind = !after[file] || after[file] === "CRASH" ? "crash" : "diverges",
                    dest = path.join(a.out, `${a.seed}-${round}-${fs.readdirSync(a.out).length}-${kind}`);
                fs.mkdirSync(dest);
                fs.copyFileSync(file, path.join(dest, path.basename(file)));
                write(path.join(dest, "flags"), flags + "\n");
                const [g, r] = await Promise.all(
                    [go, rs].map((b) => runAsync([b, "dump", file, flags], { check: false, timeout: 300 })),
                );
                write(
                    path.join(dest, "diff.txt"),
                    unified(g.stdout + g.stderr.slice(-4000), r.stdout + r.stderr.slice(-4000), "go", "rust"),
                );
                console.log(`  ${kind}: ${dest}`);
            }
            total += lines.length;
            console.log(`round ${round + 1}/${a.rounds}: ${total} inputs, ${findings} findings`);
        } finally {
            fs.rmSync(work, { recursive: true, force: true });
        }
    }
    console.log(`parser fuzz: ${total} inputs, ${compared} compared, ${findings} findings (seed ${a.seed})`);
    requireValue(compared >= 0.9 * total, `only ${compared} of ${total} inputs got a Go hash`);
    process.exitCode = findings ? 1 : 0;
});
