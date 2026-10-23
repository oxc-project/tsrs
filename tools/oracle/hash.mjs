import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { ROOT, args, read, write, walk, run, runAsync, mapLimit, requireValue } from "../lib/cli.mjs";
import { unified } from "../lib/diff.mjs";
export async function runHash(binary, lines, mode) {
    const p = await runAsync([binary, "hash", ...(mode ? [mode] : [])], {
        input: lines.join("\n") + "\n",
        check: false,
        timeout: 600,
    });
    return Object.fromEntries(
        p.stdout
            .trimEnd()
            .split("\n")
            .filter(Boolean)
            .map((row) => {
                const i = row.indexOf(" ");
                return [row.slice(i + 1), row.slice(0, i)];
            }),
    );
}
export async function hashAll(binary, lines, jobs, mode) {
    const chunks = Array.from({ length: Math.ceil(lines.length / 200) }, (_, i) => lines.slice(i * 200, (i + 1) * 200));
    const result = Object.assign({}, ...(await mapLimit(chunks, jobs, (c) => runHash(binary, c, mode))));
    await mapLimit(
        lines.filter((l) => !Object.hasOwn(result, l.split("\t")[0])),
        jobs,
        async (l) => {
            const p = l.split("\t")[0],
                r = await runHash(binary, [l], mode);
            result[p] = r[p] ?? "CRASH";
        },
    );
    return result;
}
export async function driver(printer) {
    const a = args(`Compare ${printer ? "printer" : "AST"} oracle hashes. <list>`, {
        jobs: os.availableParallelism(),
        diffs: 10,
        out: null,
        "rust-only": false,
        ...(printer ? { mode: "default" } : { "make-tests": null, "make-libs": false }),
    });
    const go =
            process.env.ORACLE_GO ??
            path.resolve(ROOT, printer ? "../../bin/tsrs-oracle-printer" : "../bin/tsrs-oracle-ast"),
        rs =
            process.env.ORACLE_RS ??
            path.join(
                ROOT,
                printer
                    ? "target/release/examples/printer_oracle"
                    : "target/parser-integrate/release/examples/ast_oracle",
            );
    if (a["make-libs"]) {
        for (const file of walk(path.join(ROOT, "ts-ref/tsc/internal/bundled/libs"), (f) => f.endsWith(".d.ts")))
            console.log(file + "\t-");
        return;
    }
    if (a["make-tests"]) {
        const cases = ["compiler", "conformance"]
            .flatMap((sub) => [...walk(path.join(ROOT, "ts-ref/tsc/testdata/tests/cases", sub))])
            .sort();
        const p = run([go, "split", path.resolve(a["make-tests"])], { input: cases.join("\n") + "\n", check: false });
        process.stderr.write(p.stderr);
        process.stdout.write(p.stdout);
        process.exitCode = p.code;
        return;
    }
    requireValue(a._.length === 1, "one list file is required");
    const lines = read(a._[0])
            .split("\n")
            .filter((l) => l.trim()),
        out =
            a.out ??
            path.join(
                ROOT,
                "target/scratch",
                printer ? "printer-pkg" : "parser-integrate",
                path.basename(a._[0]) + ".out",
            ),
        suffix = printer ? `.${a.mode}` : "",
        cache = path.join(out, `go${suffix}.hashes`);
    const before =
        a["rust-only"] && fs.existsSync(cache)
            ? Object.fromEntries(
                  read(cache)
                      .trimEnd()
                      .split("\n")
                      .map((l) => {
                          const i = l.indexOf(" ");
                          return [l.slice(i + 1), l.slice(0, i)];
                      }),
              )
            : await hashAll(go, lines, a.jobs, a.mode);
    write(
        cache,
        Object.entries(before)
            .map(([k, v]) => `${v} ${k}\n`)
            .join(""),
    );
    const after = await hashAll(rs, lines, a.jobs, a.mode),
        bad = [],
        panics = [],
        crashes = [],
        nonUTF8 = [];
    let skipped = 0,
        goPanics = 0,
        total = 0;
    for (const line of lines) {
        const p = line.split("\t")[0];
        if (!Object.hasOwn(before, p)) continue;
        if (printer && before[p] === "SKIP" && after[p] === "SKIP") {
            skipped++;
            continue;
        }
        total++;
        if (before[p] === "PANIC") goPanics++;
        if (before[p] === after[p]) continue;
        if (printer) bad.push(line);
        else {
            const bytes = fs.readFileSync(p);
            let utf8 = true;
            if (!((bytes[0] === 255 && bytes[1] === 254) || (bytes[0] === 254 && bytes[1] === 255))) {
                try {
                    new TextDecoder("utf-8", { fatal: true }).decode(bytes);
                } catch {
                    utf8 = false;
                }
            }
            if (!utf8) nonUTF8.push(line);
            else if (after[p] === "PANIC") panics.push(line);
            else if (!after[p] || after[p] === "CRASH") crashes.push(line);
            else bad.push(line);
        }
    }
    const failures = [...panics, ...crashes, ...bad];
    write(path.join(out, `mismatches${suffix}.list`), failures.map((l) => l + "\n").join(""));
    if (!printer) write(path.join(out, "non_utf8.list"), nonUTF8.map((l) => l + "\n").join(""));
    console.log(
        printer
            ? `${a._[0]} [${a.mode}]: ${total - bad.length}/${total} identical (${skipped} skipped with parse errors, ${goPanics} Go panics), ${bad.length} mismatches`
            : `${a._[0]}: ${total - failures.length - nonUTF8.length}/${total} identical, ${bad.length} mismatches, ${panics.length} panics, ${crashes.length} crashes, ${nonUTF8.length} non-UTF-8 mismatches`,
    );
    for (const line of failures.slice(0, a.diffs)) {
        const [file, flags = "-"] = line.split("\t"),
            command = printer ? ["print", a.mode, file, flags] : ["dump", file, flags];
        const [g, r] = await Promise.all(
            [go, rs].map((b) => runAsync([b, ...command], { check: false, timeout: 600 })),
        );
        console.log(
            `=== ${line}\n${unified(g.stdout + g.stderr.slice(-2000), r.stdout + r.stderr.slice(-2000), "go", "rust")
                .split("\n")
                .slice(0, printer ? 60 : 80)
                .join("\n")}`,
        );
    }
}
