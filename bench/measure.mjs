#!/usr/bin/env node
// Measure the child with system time so Node's heap is excluded from peak RSS.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { run, runAsync, writeJSON, requireValue, isMain, main } from "../tools/lib/cli.mjs";
const timeArgs = (file) => (process.platform === "darwin" ? ["-l", "-o", file] : ["-v", "-o", file]);
function metrics(file, started) {
    const text = fs.existsSync(file) ? fs.readFileSync(file, "utf8") : "";
    const get = (re) => {
        const m = text.match(re);
        return m ? Number(m[1]) : null;
    };
    const mac = process.platform === "darwin";
    const rss = get(mac ? /(\d+)\s+maximum resident set size/ : /Maximum resident set size \(kbytes\):\s*(\d+)/);
    requireValue(rss > 0, `system time did not report peak RSS: ${text.trim() || "no measurements"}`);
    return {
        wall_s: (performance.now() - started) / 1000,
        max_rss_bytes: rss * (mac ? 1 : 1024),
        peak_footprint_bytes: mac ? get(/(\d+)\s+peak memory footprint/) : null,
        user: get(mac ? /([\d.]+) user/ : /User time \(seconds\):\s*([\d.]+)/),
        sys: get(mac ? /([\d.]+) sys/ : /System time \(seconds\):\s*([\d.]+)/),
        minflt: get(mac ? /(\d+)\s+page reclaims/ : /Minor \(reclaiming a frame\) page faults:\s*(\d+)/),
        majflt: get(mac ? /(\d+)\s+page faults/ : /Major \(requiring I\/O\) page faults:\s*(\d+)/),
        nvcsw: get(mac ? /(\d+)\s+voluntary context switches/ : /Voluntary context switches:\s*(\d+)/),
        nivcsw: get(mac ? /(\d+)\s+involuntary context switches/ : /Involuntary context switches:\s*(\d+)/),
    };
}
export function measure(command, { cwd, env, timeout = 900, instructions = false } = {}) {
    requireValue(command.length > 0, "a command is required");
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-measure-")),
        timing = path.join(dir, "time"),
        counters = path.join(dir, "perf");
    let count = null,
        error,
        result;
    let started = performance.now();
    try {
        if (instructions && process.platform === "linux") {
            result = run(
                [
                    "/usr/bin/time",
                    ...timeArgs(timing),
                    "perf",
                    "stat",
                    "-x",
                    ";",
                    "-o",
                    counters,
                    "-e",
                    "instructions:u",
                    "--",
                    ...command,
                ],
                { cwd, env, timeout, check: false },
            );
            const data = fs.existsSync(counters) ? fs.readFileSync(counters, "utf8") : "";
            const m = data.match(/^(\d+);[^\n]*?;instructions:u;/m);
            if (m) count = Number(m[1]);
            else {
                error = data.trim() || result.stderr.trim() || "instruction counter unavailable";
                // Counter setup errors occur before exec; run the command without perf in that case.
                if (
                    /permission|not supported|not found|No such file|Access to performance|perf_event_paranoid/i.test(
                        error,
                    ) &&
                    !result.stdout
                )
                    result = undefined;
            }
        } else if (instructions)
            error = `instruction counters require Linux/perf (running on ${process.platform}/${process.arch})`;
        if (!result) {
            started = performance.now();
            result = run(["/usr/bin/time", ...timeArgs(timing), ...command], { cwd, env, timeout, check: false });
        }
        return { ...result, ...metrics(timing, started), instructions: count, ...(error ? { error } : {}) };
    } finally {
        fs.rmSync(dir, { recursive: true, force: true });
    }
}
// Sampling uses the actual compiler PID: the shell writes its PID and then execs the command.
export async function measureAsync(command, { cwd, env, timeout = 900, sample, sampleMs = 10 } = {}) {
    requireValue(command.length > 0 && sampleMs > 0, "a command and a positive sampling interval are required");
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-measure-")),
        timing = path.join(dir, "time"),
        pidfile = path.join(dir, "pid");
    const started = performance.now();
    let timer;
    try {
        const wrapper = sample
            ? ["/bin/sh", "-c", 'echo $$ > "$1"; shift; exec "$@"', "tsrs-measure", pidfile, ...command]
            : command;
        if (sample)
            timer = setInterval(() => {
                if (fs.existsSync(pidfile))
                    sample(Number(fs.readFileSync(pidfile, "utf8")), (performance.now() - started) / 1000);
            }, sampleMs);
        const result = await runAsync(["/usr/bin/time", ...timeArgs(timing), ...wrapper], {
            cwd,
            env,
            timeout,
            check: false,
        });
        return { ...result, ...metrics(timing, started) };
    } finally {
        clearInterval(timer);
        fs.rmSync(dir, { recursive: true, force: true });
    }
}
export function cli(instructions) {
    const a = process.argv.slice(2);
    if (a.includes("--help") || a.includes("-h")) {
        console.log(`Usage: node bench/${instructions ? "count" : "measure"}.mjs <out.json> -- <command> [args...]`);
        return;
    }
    requireValue(a.length >= 3 && a[1] === "--", "usage: <out.json> -- <command> [args...]");
    const r = measure(a.slice(2), { instructions });
    process.stdout.write(r.stdout);
    process.stderr.write(r.stderr);
    writeJSON(
        a[0],
        instructions
            ? {
                  instructions: r.instructions,
                  max_rss_bytes: r.max_rss_bytes,
                  exit: r.code,
                  ...(r.error ? { error: r.error } : {}),
              }
            : {
                  wall_s: r.wall_s,
                  maxrss: r.max_rss_bytes / (process.platform === "darwin" ? 1 : 1024),
                  status: r.code << 8,
              },
    );
    process.exitCode = r.code;
}
if (isMain(import.meta.url)) main(() => cli(false));
