#!/usr/bin/env node
import path from "node:path";
import { args, required, write, writeJSON, read, runAsync, requireValue, main } from "../lib/cli.mjs";
import {
    projects,
    command,
    timed,
    publicRun,
    variants,
    withVariant,
    baseEnv,
    positiveList,
    report,
    machineInfo,
    procSample,
} from "./common.mjs";
main(async () => {
    const a = args("Measure checker residency and optional sampled memory splits (Linux).", {
        tsrs: null,
        work: null,
        out: null,
        projects: "t3code-server,formbricks-web,vscode",
        checkers: "32",
        reps: 0,
        "rep-checkers": "",
        variant: [],
        split: false,
        "sample-ms": 10,
    });
    required(a, "tsrs", "work", "out");
    requireValue(
        Number.isInteger(a.reps) && a.reps >= 0 && a["sample-ms"] > 0,
        "invalid repetitions or sampling interval",
    );
    requireValue(!a.split || process.platform === "linux", "--split sampling requires Linux /proc");
    const vs = variants(a.variant, ["base:"]),
        ps = projects(a.projects, a.work),
        ks = positiveList(a.checkers),
        out = path.resolve(a.out),
        env = baseEnv(true),
        results = [],
        reps = Object.fromEntries(
            a["rep-checkers"]
                .split(",")
                .filter(Boolean)
                .map((v) => v.split(":").map(Number)),
        );
    requireValue(
        Object.entries(reps).every(([k, v]) => Number(k) > 0 && Number.isInteger(v) && v >= 0),
        "invalid --rep-checkers",
    );
    let md = `# Slack probe\n\n${machineInfo().label}\n\n`;
    writeJSON(path.join(out, "machine.json"), machineInfo());
    for (const p of ps)
        for (const k of ks) {
            const cmd = command(a.tsrs, p, k);
            await runAsync(cmd, { cwd: p.cwd, env, check: false });
            if (a.split)
                for (const v of vs) {
                    const { cmd: vcmd, env: venv } = withVariant(cmd, v, { ...env, TSRS_MEM_SPLIT: "1" }),
                        samples = [],
                        log = path.join(out, "runs", `split-${p.name}-c${k}-${v.name}.txt`),
                        r = await timed(vcmd, {
                            cwd: p.cwd,
                            env: venv,
                            log,
                            sampleMs: a["sample-ms"],
                            sample: (pid, t) => {
                                const s = procSample(pid, t, true);
                                if (s) samples.push(s);
                            },
                        });
                    const peak = samples.reduce((b, s) => (!b || s.rss > b.rss ? s : b), null),
                        value = {
                            ...publicRun(r),
                            peak_sample: peak,
                            samples: samples.length,
                            max_threads: samples.length ? Math.max(...samples.map((s) => s.threads)) : null,
                        };
                    writeJSON(log.replace(/\.txt$/, ".json"), value);
                    md += `### ${p.name} c${k} ${v.name}\n\nPeak ${(r.maxrss_kib / 1048576).toFixed(3)} GiB; sampled peak ${peak?.rss ?? "n/a"} KiB, ${peak?.huge ?? "n/a"} KiB huge pages, ${samples.length} samples.\n\n\`\`\`\n${read(
                        log.replace(/\.txt$/, ".err"),
                    )
                        .split("\n")
                        .filter((l) => l.startsWith("tsrs mem split") || l.startsWith("  "))
                        .join("\n")}\n\`\`\`\n\n`;
                    requireValue(r.ok, `split run failed: ${log}`);
                }
            for (let rep = 0; rep < (reps[k] ?? a.reps); rep++)
                for (const v of vs) {
                    const { cmd: vcmd, env: venv } = withVariant(cmd, v, env),
                        samples = [],
                        r = await timed(vcmd, {
                            cwd: p.cwd,
                            env: venv,
                            log: path.join(out, "runs", `${p.name}-c${k}-${v.name}-rep${rep}.txt`),
                            sampleMs: 50,
                            sample: (pid, t) => {
                                const s = procSample(pid, t, false);
                                if (s) samples.push(s);
                            },
                        });
                    results.push({
                        project: p.name,
                        checkers: k,
                        variant: v.name,
                        rep,
                        ...publicRun(r),
                        ...(samples.length
                            ? {
                                  peak_sample: samples.reduce((b, s) => (s.rss > b.rss ? s : b)),
                                  samples: samples.length,
                                  max_threads: Math.max(...samples.map((s) => s.threads)),
                              }
                            : {}),
                    });
                    console.error(`${p.name} c${k} ${v.name} rep${rep}: ${r.wall.toFixed(3)}s, exit ${r.exit}`);
                }
        }
    writeJSON(path.join(out, "results.json"), results);
    md += report(
        results,
        vs.map((v) => v.name),
    );
    write(path.join(out, "summary.md"), md);
    console.log(md);
    process.exitCode = results.every((r) => r.ok) ? 0 : 1;
});
