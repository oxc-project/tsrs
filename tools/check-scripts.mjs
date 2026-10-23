#!/usr/bin/env node
// Syntax-check tooling without executing generators, network jobs, compilers or deployment hooks.
import fs from "node:fs";
import path from "node:path";
import { stripTypeScriptTypes } from "node:module";
import { ROOT, args, json, main, run, walk, requireValue } from "./lib/cli.mjs";
main(() => {
    args("Check JavaScript and shell syntax and root package script paths.");
    const roots = [
        "tools",
        "tasks/determinism",
        "bench",
        "upstream/tools",
        ".github/scripts",
        "npm",
        "crates/tsrs_api_codec/gen",
        "crates/tsrs_api/src/checker/testdata/go_probe",
        "crates/tsrs_api_transport/tests/fixtures",
        "crates/tsrs_ls/testdata/refs_smoke",
    ];
    const files = new Set(
        roots.flatMap((dir) => [
            ...walk(
                path.join(ROOT, dir),
                (f) => /\.(js|mjs|mts|sh)$/.test(f),
                (d) => !["node_modules", ".work", "fixtures", "vendor"].includes(path.basename(d)),
            ),
        ]),
    );
    const pkg = json(path.join(ROOT, "package.json"));
    for (const [name, command] of Object.entries(pkg.scripts))
        for (const m of command.matchAll(/(?:^|&&\s*)(?:node|bash)\s+(?:(?:--[^\s]+)\s+)*([^\s]+)/g)) {
            const file = m[1];
            if (file.includes("*")) continue;
            requireValue(fs.existsSync(path.resolve(ROOT, file)), `${name}: missing script ${file}`);
            files.add(path.resolve(ROOT, file));
        }
    for (const file of files) {
        if (/\.[cm]?ts$/.test(file)) {
            const source = stripTypeScriptTypes(fs.readFileSync(file, "utf8"), { mode: "strip", sourceUrl: file });
            run([process.execPath, "--input-type=module", "--check"], { input: source });
        } else run(file.endsWith(".sh") ? ["bash", "-n", file] : [process.execPath, "--check", file]);
    }
    const python = roots.flatMap((dir) => [
        ...walk(
            path.join(ROOT, dir),
            (f) => f.endsWith(".py"),
            (d) => !["node_modules", ".work", "vendor"].includes(path.basename(d)),
        ),
    ]);
    requireValue(!python.length, "Python scripts remain: " + python.map((f) => path.relative(ROOT, f)).join(", "));
    console.log(`Checked ${files.size} script files and ${Object.keys(pkg.scripts).length} root commands.`);
});
