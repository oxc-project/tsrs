#!/usr/bin/env node
import { performance } from "node:perf_hooks";
import { main, sleep, random, requireValue } from "../lib/cli.mjs";
import { WatchSession, positional, STRINGS, SIG_OLD, SIG_NEW, normalized, equal } from "./session.mjs";
main(async () => {
    const a = positional("Usage: editduringjob <compiler> <project> [rounds]");
    if (!a) return;
    const rounds = Number(a[2] ?? 8);
    requireValue(Number.isInteger(rounds) && rounds > 0, "rounds must be positive");
    const s = new WatchSession(a[0], a[1]),
        rng = random(7);
    let ok = true;
    try {
        let text = s.remember(STRINGS);
        requireValue(text.includes(SIG_OLD), "expected signature is absent from strings.ts");
        await s.cycles({ first: true });
        for (let i = 0; i < rounds; i++) {
            await sleep(rng() * 1200);
            const declaration = i % 3 === 2;
            text = declaration
                ? text.includes(SIG_OLD)
                    ? text.replace(SIG_OLD, SIG_NEW)
                    : text.replace(SIG_NEW, SIG_OLD)
                : text + `\n// edit ${i}\n`;
            const t = performance.now();
            s.edit(STRINGS, () => text);
            const c = (await s.cycles({ first: true }))[0],
                same = equal(normalized(c.lines).sort(), normalized(await s.fresh()).sort());
            ok &&= same;
            console.log(
                `${declaration ? "declaration change" : "comment"} detected ${(c.detected - t).toFixed(1)} ms done ${(c.done - t).toFixed(0)} ms ${same ? "SAME" : "DIFF"}`,
            );
        }
    } finally {
        await s.close();
    }
    console.log(ok ? "ALL SAME" : "MISMATCH");
    process.exitCode = ok ? 0 : 1;
});
