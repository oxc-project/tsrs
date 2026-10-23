// Shared project selection, measurements and reports for local performance probes.
import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { ROOT, json, read, write, requireValue, median } from "../lib/cli.mjs";
import { projectPath, selectProjects, machine, which, TIMED_FLAGS } from "../../bench/lib.mjs";
import { measureAsync } from "../../bench/measure.mjs";
import { diagnostics, CRASH_RE } from "../ci/compare_diagnostics.mjs";
export { diagnostics, median, TIMED_FLAGS, which };
export const delta = (base, value) => (base && value != null ? (value / base - 1) * 100 : null);
export const pct = (base, value) =>
    delta(base, value) === null ? "n/a" : `${delta(base, value) >= 0 ? "+" : ""}${delta(base, value).toFixed(1)}%`;
export const fmt = (v, n = 3) => (v == null ? "n/a" : v.toFixed(n));
export const hash = (text) => createHash("sha256").update(diagnostics(text).join("\n")).digest("hex").slice(0, 16);
export const baseEnv = (mimalloc = false) =>
    Object.fromEntries(
        Object.entries(process.env).filter(
            ([k]) => !k.startsWith("TSRS_") && (!mimalloc || !k.startsWith("MIMALLOC_")),
        ),
    );
export function positiveList(text) {
    const values = text.split(",").map(Number);
    requireValue(
        values.length && values.every((n) => Number.isInteger(n) && n > 0),
        "checker counts must be positive integers",
    );
    return values;
}
export function projects(selection, work) {
    const cfg = json(path.join(ROOT, "bench/projects.json"));
    return selectProjects(cfg, selection).map((p) => ({ ...p, ...projectPath(p, path.resolve(work)) }));
}
export const machineInfo = () => machine(false, null);
export function command(binary, project, checkers, single = true) {
    return [
        path.resolve(binary),
        "-p",
        project.config,
        ...TIMED_FLAGS,
        ...(checkers === "default"
            ? []
            : checkers === 1 && single
              ? ["--singleThreaded"]
              : ["--checkers", String(checkers)]),
    ];
}
export function envSpec(text) {
    return Object.fromEntries(
        text
            .split(";")
            .filter(Boolean)
            .map((kv) => {
                const i = kv.indexOf("=");
                requireValue(i > 0, `invalid environment override: ${kv}`);
                return [kv.slice(0, i), kv.slice(i + 1)];
            }),
    );
}
export function variants(values, defaults) {
    const result = (values.length ? values : defaults).map((value) => {
        const i = value.indexOf(":");
        requireValue(i > 0, "variant must be name:K=V;K=V");
        return { name: value.slice(0, i), env: envSpec(value.slice(i + 1)) };
    });
    requireValue(
        new Set(result.map((v) => v.name)).size === result.length && result.every((v) => /^[\w.-]+$/.test(v.name)),
        "variant names must be unique and contain only letters, digits, dots, underscores or hyphens",
    );
    return result;
}
export function withVariant(cmd, variant, env = baseEnv()) {
    const { BIN, ...extra } = variant.env;
    return { cmd: BIN ? [path.resolve(BIN), ...cmd.slice(1)] : cmd, env: { ...env, ...extra } };
}
export function procSample(pid, t, rollup) {
    try {
        const status = read(`/proc/${pid}/status`),
            field = (text, name) => Number(text.match(new RegExp(`^${name}:\\s+(\\d+)`, "m"))?.[1] ?? 0);
        const s = { t, rss: field(status, "VmRSS"), hwm: field(status, "VmHWM"), threads: field(status, "Threads") };
        if (rollup) {
            const smaps = read(`/proc/${pid}/smaps_rollup`);
            s.huge = field(smaps, "AnonHugePages");
            s.anon = field(smaps, "Anonymous");
        }
        return s;
    } catch {
        return null;
    }
}
export function vmstat() {
    try {
        return Object.fromEntries(
            read("/proc/vmstat")
                .trim()
                .split("\n")
                .map((l) => {
                    const [k, v] = l.split(/\s+/);
                    return [k, Number(v)];
                }),
        );
    } catch {
        return {};
    }
}
export async function timed(cmd, { cwd, env, log, timeout = 900, sample, sampleMs } = {}) {
    const before = vmstat(),
        r = await measureAsync(cmd, { cwd, env, timeout, sample, sampleMs }),
        after = vmstat();
    const text = r.stdout + "\n" + r.stderr;
    if (log) {
        write(log, r.stdout);
        write(log.replace(/\.[^.]+$/, "") + ".err", r.stderr);
    }
    return {
        exit: r.code,
        wall: r.wall_s,
        wall_s: r.wall_s,
        user: r.user,
        user_s: r.user,
        sys: r.sys,
        sys_s: r.sys,
        cpu: r.user + r.sys,
        maxrss_kib: r.max_rss_bytes / 1024,
        peak_rss_bytes: r.max_rss_bytes,
        rss_b: r.max_rss_bytes,
        minflt: r.minflt,
        majflt: r.majflt,
        nvcsw: r.nvcsw,
        nivcsw: r.nivcsw,
        diagnostics: hash(text),
        errors: diagnostics(text).filter((l) => /error TS\d+:/.test(l)).length,
        ok: [0, 1, 2].includes(r.code) && !CRASH_RE.test(text),
        vmstat: Object.fromEntries(Object.entries(after).map(([k, v]) => [k, v - (before[k] ?? 0)])),
        stats: text.split("\n").find((l) => l.startsWith("tsrs: leaf files")) ?? "",
        log,
        _text: text.split(/\r?\n/),
        _diagnostics: diagnostics(text),
    };
}
export const publicRun = (r) => Object.fromEntries(Object.entries(r).filter(([k]) => !k.startsWith("_")));
export function table(headers, rows) {
    return [
        "| " + headers.join(" | ") + " |",
        "| " + headers.map(() => "---").join(" | ") + " |",
        ...rows.map((r) => "| " + r.join(" | ") + " |"),
    ].join("\n");
}
export function report(results, variantNames) {
    const rows = [];
    for (const p of new Set(results.map((r) => r.project)))
        for (const k of new Set(results.filter((r) => r.project === p).map((r) => r.checkers))) {
            const cell = results.filter((r) => r.project === p && r.checkers === k && !r.stats),
                refs = cell.filter((r) => r.variant === variantNames[0]);
            for (const v of variantNames) {
                const rs = cell.filter((r) => r.variant === v);
                if (!rs.length) continue;
                rows.push([
                    p,
                    k,
                    v,
                    rs.length,
                    fmt(median(rs.map((r) => r.wall))),
                    pct(median(refs.map((r) => r.wall)), median(rs.map((r) => r.wall))),
                    fmt(median(rs.map((r) => r.wall / refs.find((b) => b.rep === r.rep).wall)), 4),
                    fmt(median(rs.map((r) => r.cpu))),
                    fmt(median(rs.map((r) => r.maxrss_kib)) / 1048576),
                    pct(median(refs.map((r) => r.maxrss_kib)), median(rs.map((r) => r.maxrss_kib))),
                    [...new Set(rs.map((r) => r.errors))].join(","),
                    new Set(cell.map((r) => `${r.exit}:${r.diagnostics}`)).size === 1 && cell.every((r) => r.ok)
                        ? "same"
                        : "DIFFERENT",
                ]);
            }
        }
    return (
        table(
            [
                "project",
                "checkers",
                "variant",
                "reps",
                "wall s",
                "Δ wall",
                "paired ratio",
                "CPU s",
                "peak GiB",
                "Δ RSS",
                "errors",
                "diagnostics/exit",
            ],
            rows,
        ) + "\n"
    );
}
export function perfBinary() {
    const dir = "/usr/lib/linux-tools";
    if (fs.existsSync(dir)) {
        const bins = fs
            .readdirSync(dir)
            .sort()
            .reverse()
            .map((d) => path.join(dir, d, "perf"));
        const bin = bins.find((f) => fs.existsSync(f));
        if (bin) return bin;
    }
    return which("perf");
}
