#!/usr/bin/env node
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { main, read, write, sleep, walk, run, requireValue } from "../lib/cli.mjs";
import { WatchSession, positional, diags, equal, differences } from "./session.mjs";
main(async () => {
    const a = positional("Usage: savepatterns <compiler> <project> <file> [many-directory]", 3);
    if (!a) return;
    const s = new WatchSession(a[0], a[1]),
        target = a[2],
        many = a[3],
        results = [],
        temporary = new Set(),
        only = process.env.ONLY;
    let renamed, branch, head, indexDir;
    const inplace = (rel, text) => write(s.file(rel), text),
        temp = (rel, suffix) => {
            const file = s.file(rel) + suffix;
            requireValue(!fs.existsSync(file), `temporary path already exists: ${file}`);
            temporary.add(file);
            return file;
        };
    const patterns = [
        ["in-place write", inplace],
        [
            "truncate, 200 ms, write",
            async (rel, text) => {
                inplace(rel, "");
                await sleep(200);
                inplace(rel, text);
            },
        ],
        [
            "4 chunked writes over 300 ms",
            async (rel, text) => {
                const fd = fs.openSync(s.file(rel), "w");
                try {
                    const data = Buffer.from(text),
                        n = Math.floor(data.length / 4) + 1;
                    for (let i = 0; i < 4; i++) {
                        fs.writeSync(fd, data.subarray(i * n, (i + 1) * n));
                        if (i < 3) await sleep(100);
                    }
                } finally {
                    fs.closeSync(fd);
                }
            },
        ],
        [
            "atomic rename",
            (rel, text) => {
                const file = temp(rel, ".tmp");
                write(file, text);
                fs.renameSync(file, s.file(rel));
                temporary.delete(file);
            },
        ],
        [
            "rename away, write, delete backup",
            (rel, text) => {
                const backup = temp(rel, "~");
                fs.renameSync(s.file(rel), backup);
                try {
                    inplace(rel, text);
                } catch (error) {
                    fs.renameSync(backup, s.file(rel));
                    throw error;
                }
                fs.rmSync(backup);
                temporary.delete(backup);
            },
        ],
    ];
    const check = async (name, act) => {
        await act();
        const cycles = await s.cycles(),
            got = diags(cycles.at(-1).lines),
            want = diags(await s.fresh(true)),
            same = equal(got, want);
        results.push({ name, cycles: cycles.length, same });
        console.log(`${name.padEnd(40)} cycles ${cycles.length} ${same ? "SAME" : "DIFF"}`);
        if (!same) differences(got, want);
    };
    const git = (argv, options = {}) => run(["git", ...argv], { cwd: s.project, ...options });
    try {
        const base = s.remember(target);
        await s.cycles();
        for (const [name, save] of patterns) {
            if (only && !name.includes(only)) continue;
            await check(name + " (add error)", () =>
                save(target, base + "\nconst savepatternsErr: number = 'savepatterns half-written';\n"),
            );
            await check(name + " (remove it)", () => save(target, base));
        }
        if (!only || "case-only rename".includes(only)) {
            const n = path.basename(target),
                swapped = path.join(
                    path.dirname(target),
                    (n[0] === n[0].toUpperCase() ? n[0].toLowerCase() : n[0].toUpperCase()) + n.slice(1),
                );
            requireValue(swapped !== target, "target must start with a cased letter");
            if (fs.existsSync(s.file(swapped)))
                requireValue(
                    fs.statSync(s.file(swapped)).ino === fs.statSync(s.file(target)).ino,
                    "case-rename destination exists",
                );
            await check("case-only rename", () => {
                fs.renameSync(s.file(target), s.file(swapped));
                renamed = swapped;
            });
            await check("case-only rename back", () => {
                fs.renameSync(s.file(swapped), s.file(target));
                renamed = null;
            });
        }
        if (many && !only) {
            const files = [...walk(s.file(many), (f) => f.endsWith(".ts") && !f.endsWith(".d.ts"))]
                .sort()
                .slice(0, 200)
                .map((f) => path.relative(s.project, f));
            for (const f of files) s.remember(f);
            await check(`formatter rewrites ${files.length} files`, () => {
                for (const f of files) inplace(f, s.remember(f) + "\n// savepatterns formatter\n");
            });
            await check(`formatter restores ${files.length} files`, () => {
                for (const f of files) inplace(f, s.remember(f));
            });
            // A unique disposable branch avoids overwriting an existing branch. Require a clean worktree before checkout.
            if (
                files.length &&
                git(["rev-parse", "--is-inside-work-tree"], { check: false }).code === 0 &&
                git(["status", "--porcelain"]).stdout.trim() === ""
            ) {
                head =
                    git(["symbolic-ref", "--short", "-q", "HEAD"], { check: false }).stdout.trim() ||
                    git(["rev-parse", "HEAD"]).stdout.trim();
                const top = git(["rev-parse", "--show-toplevel"]).stdout.trim();
                indexDir = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-watch-index-"));
                const env = { ...process.env, GIT_INDEX_FILE: path.join(indexDir, "index") };
                git(["read-tree", "HEAD"], { env });
                for (const f of files) {
                    const blob = git(["hash-object", "-w", "--stdin"], {
                        input: s.remember(f) + "\n// savepatterns branch\n",
                    }).stdout.trim();
                    git(["update-index", "--cacheinfo", `100644,${blob},${path.relative(top, s.file(f))}`], { env });
                }
                const tree = git(["write-tree"], { env }).stdout.trim(),
                    commit = git(["commit-tree", tree, "-p", "HEAD", "-m", "savepatterns"]).stdout.trim();
                branch = `tsrs-savepatterns-${process.pid}-${Date.now()}`;
                git(["branch", branch, commit]);
                await check(`git checkout touching ${files.length} files`, () => git(["checkout", "--quiet", branch]));
                await check("git checkout back", () => git(["checkout", "--quiet", head]));
                git(["branch", "-D", branch]);
                branch = null;
            } else console.log("Git checkout pattern skipped: requires tracked files and a clean worktree.");
        }
    } finally {
        try {
            if (renamed && fs.existsSync(s.file(renamed))) fs.renameSync(s.file(renamed), s.file(target));
            if (branch && head) {
                git(["checkout", "--quiet", head]);
                git(["branch", "-D", branch]);
            }
            for (const f of temporary) fs.rmSync(f, { force: true });
            if (indexDir) fs.rmSync(indexDir, { recursive: true, force: true });
        } finally {
            await s.close();
        }
    }
    const bad = results.filter((r) => r.cycles !== 1 || !r.same);
    console.log(bad.length ? `${bad.length} BAD: ${bad.map((r) => r.name).join(", ")}` : "ALL ONE CYCLE, ALL SAME");
    process.exitCode = bad.length ? 1 : 0;
});
