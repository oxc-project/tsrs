#!/usr/bin/env node
import path from "node:path";
import { args, read, run, requireValue, main } from "./lib/cli.mjs";
main(() => {
    const a = args("Write a deterministic 2,000-file sample for tsrs-test types-dump --sample. <project-dir>");
    requireValue(a._.length === 1, "one project directory is required");
    const root = path.resolve(a._[0]);
    const files = run(["git", "ls-files", "-z", "*.ts", "*.tsx"], { cwd: root })
            .stdout.split("\0")
            .filter((f) => f && !f.endsWith(".d.ts") && !/^(scripts|client)\//.test(f))
            .sort(),
        calls = new Map();
    for (const f of files) {
        let text;
        try {
            text = read(path.join(root, f));
        } catch {
            continue;
        }
        if (/from ['"]zod['"]/.test(text)) calls.set(f, [...text.matchAll(/\bz\./g)].length);
    }
    const groups = {
            zod: [...calls.keys()].sort((a, b) => calls.get(b) - calls.get(a) || (a < b ? -1 : 1)),
            "orm-entities": files.filter((f) => f.includes("/orm/entities/")),
            workflows: files.filter((f) => f.toLowerCase().includes("workflow")),
            router: files.filter((f) => f.startsWith("src/router/")),
        },
        quotas = [
            ["zod", 500],
            ["orm-entities", 450],
            ["workflows", 400],
            ["router", 450],
            ["rest", 200],
        ],
        seen = new Set(),
        chosen = [];
    const stride = (items, n) =>
        items.length <= n ? items : Array.from({ length: n }, (_, i) => items[Math.floor((i * items.length) / n)]);
    for (const [name, n] of quotas) {
        const pool = (groups[name]?.length ? groups[name] : files).filter((f) => !seen.has(f)),
            heavy = name === "zod" ? pool.slice(0, Math.floor((n * 3) / 5)) : [],
            picked = [
                ...heavy,
                ...stride(
                    pool.filter((f) => !heavy.includes(f)),
                    n - heavy.length,
                ),
            ];
        for (const f of picked) {
            seen.add(f);
            chosen.push([name, f]);
        }
    }
    console.log("# tsrs-test types-dump --sample list; regenerate with tools/project-types-sample.mjs");
    console.log(
        "# groups: " + quotas.map(([name]) => `${name} ${chosen.filter(([g]) => g === name).length}`).join(", "),
    );
    console.log(
        chosen
            .map(([, f]) => f)
            .sort()
            .join("\n"),
    );
});
