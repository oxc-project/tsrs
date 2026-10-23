#!/usr/bin/env node
import { args, read, main } from "../lib/cli.mjs";
function firstFrame(text) {
    const frames = [...text.matchAll(/#\d+ (\S+) (\S+)/g)].map((m) => [m[1].replace(/::h[0-9a-f]{16}$/, ""), m[2]]);
    const f = frames.find(([fn]) => fn.split("<")[0].includes("tsrs"));
    return f ? `${f[0].slice(0, 110)} (${f[1].split("/").at(-1)})` : (frames[0]?.[0].slice(0, 110) ?? "?");
}
main(() => {
    const a = args("Group ThreadSanitizer reports by kind and first tsrs frame. <log>...", { top: 10 }),
        groups = new Map();
    for (const file of a._)
        for (const report of read(file).split("==================")) {
            const m = report.match(/WARNING: ThreadSanitizer: ([^(\n]+)/);
            if (!m) continue;
            const parts = report.split(/\n\s*Previous /),
                key = [m[1].trim(), firstFrame(parts[0]), parts[1] ? firstFrame(parts[1]) : "-"],
                id = JSON.stringify(key);
            if (!groups.has(id)) groups.set(id, { key, n: 0, example: report.trim() });
            groups.get(id).n++;
        }
    console.log(`${[...groups.values()].reduce((s, g) => s + g.n, 0)} reports, ${groups.size} unique`);
    for (const {
        key: [kind, now, before],
        n,
        example,
    } of [...groups.values()].sort((a, b) => b.n - a.n).slice(0, a.top))
        console.log(
            `\n${n} x ${kind}\n  access:          ${now}\n  earlier access:  ${before}\n\n${example.split("\n").slice(0, 40).join("\n")}`,
        );
    process.exitCode = groups.size ? 1 : 0;
});
