#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { args, required, write, writeJSON, runAsync, main, requireValue } from "../lib/cli.mjs";
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
    perfBinary,
    which,
    table,
    median,
    fmt,
} from "./common.mjs";
main(async () => {
    const a = args("Measure leaf-region variants, VM counters, perf and syscall profiles.", {
        tsrs: null,
        work: null,
        out: null,
        projects: "vscode,formbricks-web",
        checkers: "1,4",
        reps: 5,
        variant: [],
        profile: [],
        strace: true,
        "perf-stat": true,
    });
    required(a, "tsrs", "work", "out");
    requireValue(Number.isInteger(a.reps) && a.reps > 0, "--reps must be positive");
    const vs = variants(a.variant, ["off:TSRS_FREE_LEAVES=0", "on:TSRS_FREE_LEAVES=1"]),
        ps = projects(a.projects, a.work),
        ks = positiveList(a.checkers),
        out = path.resolve(a.out),
        env = baseEnv(),
        results = [],
        machine = machineInfo();
    writeJSON(path.join(out, "machine.json"), machine);
    for (const p of ps)
        for (const k of ks) {
            const cmd = command(a.tsrs, p, k);
            await runAsync(cmd, { cwd: p.cwd, env, check: false });
            for (let rep = 0; rep < a.reps; rep++)
                for (const v of vs) {
                    const { cmd: vcmd, env: venv } = withVariant(cmd, v, env),
                        r = await timed(vcmd, {
                            cwd: p.cwd,
                            env: venv,
                            log: path.join(out, "runs", `${p.name}-c${k}-${v.name}-rep${rep}.txt`),
                        });
                    results.push({ project: p.name, checkers: k, variant: v.name, rep, ...publicRun(r) });
                    console.error(`${p.name} c${k} ${v.name} rep${rep}: ${r.wall.toFixed(3)}s, exit ${r.exit}`);
                }
        }
    writeJSON(path.join(out, "results.json"), results);
    const counters = ["minflt", "majflt", "nvcsw", "nivcsw", "thp_fault_alloc", "compact_stall", "nr_tlb_remote_flush"],
        rows = [];
    for (const p of ps)
        for (const k of ks)
            for (const v of vs) {
                const rs = results.filter((r) => r.project === p.name && r.checkers === k && r.variant === v.name);
                rows.push([
                    p.name,
                    k,
                    v.name,
                    ...counters.map((c) => fmt(median(rs.map((r) => r[c] ?? r.vmstat[c])), 0)),
                ]);
            }
    let md =
        `# Leaf probe\n\n${machine.label}\n\n` +
        report(
            results,
            vs.map((v) => v.name),
        ) +
        "\n" +
        table(["project", "checkers", "variant", ...counters], rows) +
        "\n";
    write(path.join(out, "summary.md"), md);
    console.log(md);
    const perf = perfBinary();
    const extra = async (title, cmd, p, env, output) => {
        const r = await runAsync(cmd, { cwd: p.cwd, env, check: false, timeout: 1800 }),
            text = fs.existsSync(output) ? fs.readFileSync(output, "utf8") : r.stdout + r.stderr;
        md += `\n### ${title}\n\n\`\`\`\n${text.slice(0, 12000)}\n\`\`\`\n`;
    };
    for (const p of ps)
        for (const k of ks)
            for (const v of vs) {
                const { cmd, env: venv } = withVariant(command(a.tsrs, p, k), v, env),
                    tag = `${p.name}-c${k}-${v.name}`;
                if (perf && a["perf-stat"]) {
                    const file = path.join(out, `perfstat-${tag}.csv`);
                    await extra(
                        `perf stat ${tag}`,
                        [
                            perf,
                            "stat",
                            "-r",
                            "2",
                            "-e",
                            "page-faults,minor-faults,dTLB-load-misses,dTLB-store-misses,context-switches,cpu-migrations,cycles:u,cycles:k,instructions:u,instructions:k",
                            "-x",
                            ",",
                            "-o",
                            file,
                            "--",
                            ...cmd,
                        ],
                        p,
                        venv,
                        file,
                    );
                }
                if (which("strace") && a.strace) {
                    const file = path.join(out, `strace-${tag}.txt`);
                    await extra(`strace ${tag}`, ["strace", "-f", "-c", "-o", file, "--", ...cmd], p, venv, file);
                }
            }
    for (const spec of a.profile) {
        requireValue(perf, "--profile requires perf");
        const [name, k = "1"] = spec.split(":"),
            p = ps.find((p) => p.name === name);
        requireValue(p, "profile project must be selected with --projects");
        const datas = [];
        for (const v of vs) {
            const { cmd, env: venv } = withVariant(command(a.tsrs, p, Number(k)), v, env),
                tag = `${name}-c${k}-${v.name}`,
                data = path.join(out, `perf-${tag}.data`);
            datas.push(data);
            await runAsync([perf, "record", "-F", "999", "-g", "-o", data, "--", ...cmd], {
                cwd: p.cwd,
                env: venv,
                check: false,
                timeout: 1800,
            });
            for (const sort of ["dso", "sym"]) {
                const r = await runAsync(
                    [
                        perf,
                        "report",
                        "-i",
                        data,
                        "--no-children",
                        "--sort",
                        sort,
                        "--stdio",
                        "-g",
                        "none",
                        "--percent-limit",
                        "0.3",
                    ],
                    { check: false },
                );
                write(path.join(out, `perf-${tag}-${sort}.txt`), r.stdout + r.stderr);
                md += `\n### perf report ${tag} ${sort}\n\n\`\`\`\n${r.stdout.slice(0, 6000)}\n\`\`\`\n`;
            }
            const flat = await runAsync(
                [
                    perf,
                    "report",
                    "-i",
                    data,
                    "--no-children",
                    "--sort",
                    "sym",
                    "--stdio",
                    "-g",
                    "none",
                    "--percent-limit",
                    "0.005",
                ],
                { check: false },
            );
            write(
                path.join(out, `perf-${tag}-kernel-callers.txt`),
                flat.stdout
                    .split("\n")
                    .filter((l) => l.includes("[k]"))
                    .join("\n"),
            );
        }
        for (const data of datas.slice(1)) {
            const r = await runAsync([perf, "diff", "--sort", "dso,sym", datas[0], data], { check: false });
            write(
                path.join(out, `perf-diff-${path.basename(datas[0])}-${path.basename(data)}.txt`),
                r.stdout + r.stderr,
            );
        }
        for (const data of datas) fs.rmSync(data, { force: true });
    }
    write(path.join(out, "summary.md"), md);
    process.exitCode = results.every((r) => r.ok) ? 0 : 1;
});
