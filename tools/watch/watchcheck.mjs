#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { main, runAsync, requireValue, walk } from "../lib/cli.mjs";
import {
    WatchSession,
    positional,
    STRINGS,
    LIFECYCLE,
    CONSUMER,
    SIG_OLD,
    SIG_NEW,
    normalized,
    equal,
    differences,
} from "./session.mjs";
const append = (text) => (s) => s + text,
    drop = (text) => (s) => s.replace(text, "");
const edits = [
    ["comment", STRINGS, append("\n// watchcheck comment\n")],
    ["type error", STRINGS, append("\nconst watchcheckErr: number = 'x';\n")],
    ["fix type error", STRINGS, drop("\nconst watchcheckErr: number = 'x';\n")],
    ["new export in hub", LIFECYCLE, append("\nexport const watchcheckX = 1;\n")],
    [
        "new file using it",
        "vs/base/common/watchcheckNew.ts",
        () => "import { watchcheckX } from './lifecycle';\nexport const y: string = watchcheckX;\n",
    ],
    [
        "fix new file",
        "vs/base/common/watchcheckNew.ts",
        () => "import { watchcheckX } from './lifecycle';\nexport const y: number = watchcheckX;\n",
    ],
    ["remove the export", LIFECYCLE, drop("\nexport const watchcheckX = 1;\n")],
    ["delete new file", "vs/base/common/watchcheckNew.ts", null],
    ["break an import", CONSUMER, (s) => "import { doesNotExist } from './strings';\n" + s],
    ["unbreak the import", CONSUMER, drop("import { doesNotExist } from './strings';\n")],
    ["export type change", STRINGS, (s) => s.replace(SIG_OLD, SIG_NEW)],
    ["export type restore", STRINGS, (s) => s.replace(SIG_NEW, SIG_OLD)],
];
main(async () => {
    const a = positional("Usage: watchcheck <compiler> <project>");
    if (!a) return;
    const s = new WatchSession(...a);
    let ok = true;
    try {
        for (const [name, rel, fn] of [["initial", null, null], ...edits]) {
            if (rel) s.edit(rel, fn);
            const cycles = await s.cycles(),
                got = normalized(cycles.at(-1).lines),
                want = normalized(await s.fresh()),
                same = equal(got, want);
            ok &&= same;
            console.log(
                `${name.padEnd(22)} cycles ${cycles.length} ${cycles.at(-1).summary} ${same ? "SAME" : "DIFF"}`,
            );
            if (!same) differences(got, want);
        }
        if (process.env.EMITCHECK) {
            await s.stop();
            const emitted = path.resolve(s.project, process.env.EMITCHECK),
                fresh = emitted + "-fresh";
            requireValue(fresh !== s.project && !s.project.startsWith(fresh + path.sep), "unsafe emit output");
            fs.rmSync(fresh, { recursive: true, force: true });
            const r = await runAsync([s.compiler, "-p", ".", "--pretty", "false", "--outDir", fresh], {
                cwd: s.project,
                check: false,
            });
            requireValue([0, 1, 2].includes(r.code), "fresh emit failed");
            const diff = await runAsync(["diff", "-rq", emitted, fresh], { check: false });
            ok &&= diff.code === 0;
            console.log(
                `emit check: ${[...walk(fresh)].length} fresh files; ${diff.code === 0 ? "IDENTICAL" : "DIFFERENT"}\n${diff.stdout.split("\n").slice(0, 10).join("\n")}`,
            );
        }
    } finally {
        await s.close();
    }
    console.log(ok ? "ALL SAME" : "MISMATCH");
    process.exitCode = ok ? 0 : 1;
});
