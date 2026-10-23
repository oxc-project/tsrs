#!/usr/bin/env node
// Small workloads used by the determinism and ThreadSanitizer shell gates.
import path from "node:path";
import { ROOT, json, write, writeJSON, requireValue, main } from "../lib/cli.mjs";
import { WatchSession } from "../watch/session.mjs";
main(async () => {
    const [mode, ...a] = process.argv.slice(2);
    if (!mode || ["--help", "-h"].includes(mode)) {
        console.log(
            "Usage: workloads project <catalog> <name> <work> | graph <output> | watch <files...> -- <command...>",
        );
        return;
    }
    if (mode === "project") {
        const [catalog, name, work] = a,
            p = json(catalog).projects.find((p) => p.name === name);
        requireValue(p, `unknown project ${name}`);
        const sub = p.suite_dir ? "suite/" + p.suite_dir : "solutions/" + p.name;
        console.log(work ? path.resolve(work, sub, p.project) : sub + "\n" + p.project);
    } else if (mode === "graph") {
        requireValue(a[0], "output directory is required");
        const root = path.resolve(a[0]),
            n = 12;
        for (let i = 0; i < n; i++) {
            const deps = Array.from({ length: i }, (_, j) => j).filter(
                (j) => j >= i - 3 && ((i + j) % 2 === 0 || j === i - 1),
            );
            writeJSON(path.join(root, `p${i}/tsconfig.json`), {
                compilerOptions: {
                    composite: true,
                    outDir: "out",
                    rootDir: "src",
                    strict: true,
                    target: "es2020",
                    module: "esnext",
                    moduleResolution: "bundler",
                    types: [],
                },
                include: ["src"],
                references: deps.map((j) => ({ path: `../p${j}` })),
            });
            for (let f = 0; f < 40; f++) {
                const imports =
                        f === 0
                            ? deps.map((j) => `import { v${j}_0 } from "../../p${j}/src/f0";\n`).join("")
                            : `import { v${i}_${f - 1} } from "./f${f - 1}";\n`,
                    use = f === 0 ? deps.map((j) => `v${j}_0`).join(" + ") || "0" : `v${i}_${f - 1}`,
                    body = Array.from(
                        { length: 15 },
                        (_, k) =>
                            `export interface S${k} { a: number; b: string; c: S${k}[] }\nexport function g${k}(x: S${k}): number { return x.a + x.c.length; }`,
                    ).join("\n");
                write(
                    path.join(root, `p${i}/src/f${f}.ts`),
                    `${imports}${body}\nexport const v${i}_${f}: number = ${use};\n`,
                );
            }
        }
        writeJSON(path.join(root, "tsconfig.json"), {
            files: [],
            references: Array.from({ length: n }, (_, i) => ({ path: `p${i}` })),
        });
    } else if (mode === "watch") {
        const sep = a.indexOf("--");
        requireValue(sep > 0 && a[sep + 1], "files and a command after -- are required");
        const files = a.slice(0, sep).map((f) => path.resolve(f)),
            cmd = a.slice(sep + 1);
        let cwd = path.dirname(files[0]);
        while (!files.every((f) => f.startsWith(cwd + path.sep))) cwd = path.dirname(cwd);
        const session = new WatchSession(cmd[0], cwd, cmd.slice(1), true);
        try {
            await session.cycles({ timeout: 600000 });
            for (let i = 0; i < files.length; i++) {
                const rel = path.relative(session.project, path.resolve(files[i])),
                    text = session.remember(rel);
                session.edit(rel, () => text + `\nexport const tsanEdit${i} = ${i};\n`);
                await session.cycles({ timeout: 600000 });
                session.edit(rel, () => text);
                await session.cycles({ timeout: 600000 });
            }
        } finally {
            await session.close();
        }
    } else throw new Error(`unknown workload ${mode}`);
});
