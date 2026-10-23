#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { args, read, write, runAsync, walk, mapLimit, requireValue, main, isMain } from "../../lib/cli.mjs";
import { referenceBinary, tsrsBinary } from "../../lib/compiler.mjs";
const harness = new Set(
    "filename allowNonTsExtensions useCaseSensitiveFileNames baselineFile includeBuiltFile fileName libFiles noErrorTruncation suppressOutputPathCheck noImplicitReferences currentDirectory symlink link noTypesAndSymbols fullEmitPaths noCheck reportDiagnostics captureSuggestions typeScriptVersion skipDefaultLibCheck outFile out"
        .toLowerCase()
        .split(" "),
);
export function parse(file) {
    const options = {},
        files = [];
    let current = null,
        lines = [];
    for (const line of read(file).match(/[^\n]*\n|[^\n]+$/g) ?? []) {
        const m = line.trimEnd().match(/^\s*\/\/\s*@(\w+)\s*:\s*(.*?)\s*$/);
        if (!m) {
            lines.push(line);
            continue;
        }
        if (m[1].toLowerCase() === "filename") {
            if (current || lines.some((l) => l.trim())) files.push([current ?? path.basename(file), lines.join("")]);
            current = m[2];
            lines = [];
        } else options[m[1]] = m[2];
    }
    files.push([current ?? path.basename(file), lines.join("")]);
    return { options, files };
}
export function variants(options) {
    let result = [{}];
    for (const [key, value] of Object.entries(options)) {
        if (harness.has(key.toLowerCase())) continue;
        const values =
            value.includes(",") && !["lib", "types", "paths", "rootdirs", "typeroots"].includes(key.toLowerCase())
                ? value.split(",").map((s) => s.trim())
                : [value];
        result = result.flatMap((r) => values.map((v) => ({ ...r, [key]: v })));
    }
    return result;
}
if (isMain(import.meta.url))
    main(async () => {
        const a = args("Compare emitted files for compiler/conformance cases. <file-or-directory>...", {
                jobs: os.availableParallelism(),
                show: false,
            }),
            go = referenceBinary(),
            rs = tsrsBinary(),
            out = process.env.OUT ?? path.join(os.tmpdir(), "emit-cases");
        requireValue(a._.length, "case files or directories are required");
        const files = a._.flatMap((f) =>
                fs.statSync(f).isDirectory() ? [...walk(f, (p) => /\.(ts|tsx|js|jsx|mts|cts)$/.test(p))] : [f],
            ).sort(),
            counts = {};
        const results = await mapLimit(files, a.jobs, async (file) => {
            const { options, files } = parse(file),
                result = [];
            for (const [i, variant] of variants(options).entries()) {
                fs.mkdirSync(out, { recursive: true });
                const work = fs.mkdtempSync(path.join(out, "case-")),
                    src = path.join(work, "src"),
                    roots = [];
                for (const [name, text] of files) {
                    const target = path.resolve(src, name.replace(/^\/+/, ""));
                    requireValue(target.startsWith(src + path.sep), "fixture path escapes scratch directory");
                    write(target, text);
                    if (!name.endsWith(".json")) roots.push(path.relative(src, target));
                }
                const flags = Object.entries(variant)
                        .filter(([k]) => !["paths", "plugins"].includes(k.toLowerCase()))
                        .flatMap(([k, v]) => [`--${k}`, v]),
                    compile = (binary, dest) =>
                        runAsync([binary, "--outDir", `../${dest}`, "--pretty", "false", ...flags, ...roots], {
                            cwd: src,
                            check: false,
                            timeout: 60,
                        });
                const g = await compile(go, "go"),
                    r = await compile(rs, "rs"),
                    tree = (dir) =>
                        new Map(
                            fs.existsSync(dir)
                                ? [...walk(dir)].map((f) => [path.relative(dir, f), fs.readFileSync(f)])
                                : [],
                        ),
                    left = tree(path.join(work, "go")),
                    right = tree(path.join(work, "rs")),
                    bad = [...new Set([...left.keys(), ...right.keys()])].filter(
                        (k) => !left.get(k)?.equals(right.get(k)),
                    );
                const status =
                        ![0, 1, 2].includes(r.code) || r.stderr.includes("panicked")
                            ? "crash"
                            : bad.length
                              ? "fail"
                              : left.size
                                ? "pass"
                                : "empty",
                    name = path.basename(file, path.extname(file)) + (i ? `.${i}` : "");
                result.push([
                    name,
                    status,
                    status === "crash" ? (r.stdout + r.stderr).slice(-200) : bad.slice(0, 3).join(","),
                ]);
                if (["pass", "empty"].includes(status)) fs.rmSync(work, { recursive: true, force: true });
            }
            return result;
        });
        for (const [name, status, detail] of results.flat()) {
            counts[status] = (counts[status] ?? 0) + 1;
            if (a.show || !["pass", "empty"].includes(status)) console.log(`${status.padEnd(6)} ${name} ${detail}`);
        }
        console.log(
            "variants: " +
                Object.entries(counts)
                    .sort()
                    .map(([k, v]) => `${k} ${v}`)
                    .join(", ") +
                `, total ${Object.values(counts).reduce((a, b) => a + b, 0)}`,
        );
        process.exitCode = counts.fail || counts.crash ? 1 : 0;
    });
