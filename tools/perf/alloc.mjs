#!/usr/bin/env node
// Compare DHAT heap profiles. Timing and RSS belong to separate, uninstrumented runs.
import fs from "node:fs";
import path from "node:path";
import { args, required, main } from "../lib/cli.mjs";
import { diagnostics, CRASH_RE } from "../ci/compare_diagnostics.mjs";
import {
    REPO,
    TIMED_FLAGS,
    catalog,
    selectProjects,
    projectPath,
    cleanEnvironment,
    exec,
    fileSha,
    sha,
    readJson,
    writeJson,
    requireValue,
    median,
    machine,
    stamp,
    MARKER,
    overlaySha,
} from "../../bench/lib.mjs";

const fields = {
    totalBlocks: "tbk",
    totalBytes: "tb",
    peakBlocks: "gbk",
    peakBytes: "gb",
    endBlocks: "ebk",
    endBytes: "eb",
};

function summarize(profile) {
    requireValue(
        profile.dhatFileVersion === 2 && profile.mode === "rust-heap",
        "Expected a DHAT Rust heap profile",
    );
    requireValue(Array.isArray(profile.pps) && Array.isArray(profile.ftbl), "Missing DHAT allocation stacks");
    const totals = Object.fromEntries(Object.keys(fields).map((k) => [k, 0]));
    const sites = new Map();
    for (const pp of profile.pps) {
        for (const [key, field] of Object.entries(fields)) {
            requireValue(Number.isSafeInteger(pp[field]) && pp[field] >= 0, `Invalid DHAT ${field}`);
            totals[key] += pp[field];
            requireValue(
                Number.isSafeInteger(totals[key]),
                `DHAT ${key} exceeds JavaScript's exact integer range`,
            );
        }
        requireValue(
            Array.isArray(pp.fs) &&
                pp.fs.every((i) => Number.isInteger(i) && typeof profile.ftbl[i] === "string"),
            "Invalid DHAT frames",
        );
        const frames = pp.fs.map((i) =>
            profile.ftbl[i].replace(/^0x[\da-f]+:\s*/i, "").replace(/::h[\da-f]{16}\b/g, ""),
        );
        // Generic std methods can have a tsrs receiver (e.g. <P<Node> as ConvertVec>::to_vec).
        // Attribute by the defining source file rather than mistaking that receiver for our code.
        const frame =
            frames.find((f) => /\((?:[^)]*[/\\])?tsrs_[^/\\]+[/\\]src[/\\]/.test(f)) ??
            frames[0] ??
            "<unknown>";
        // Line numbers and binary addresses change between builds. Keep the representative full stack below.
        const site = frame.replace(/ \(.*$/, "");
        const row = sites.get(site) ?? {
            site,
            totalBlocks: 0,
            totalBytes: 0,
            peakBytes: 0,
            endBytes: 0,
            frames,
        };
        row.totalBlocks += pp.tbk;
        row.totalBytes += pp.tb;
        row.peakBytes += pp.gb;
        row.endBytes += pp.eb;
        sites.set(site, row);
    }
    requireValue(totals.totalBlocks > 0, "DHAT recorded no allocations");
    return {
        ...totals,
        sites: [...sites.values()].sort(
            (a, b) => b.totalBlocks - a.totalBlocks || b.totalBytes - a.totalBytes,
        ),
    };
}

main(async () => {
    const a = args(
        "Profile CLI heap allocations with DHAT; projects must already be prepared with vp run bench prepare.",
        {
            base: null,
            new: null,
            "base-revision": "unknown",
            "new-revision": "unknown",
            projects: "webpack,typeorm,xstate-main,Compiler-Unions,vscode",
            modes: "reference,fast",
            reps: 3,
            frames: 4,
            timeout: 1800,
            "work-dir": path.join(REPO, "bench/.work"),
            out: path.join(REPO, "bench/.work/dhat", stamp()),
        },
    );
    required(a, "new");
    requireValue(!a._.length, "Unexpected positional arguments");
    requireValue(Number.isInteger(a.reps) && a.reps > 0 && a.timeout > 0, "Invalid repetitions or timeout");
    requireValue(
        Number.isSafeInteger(a.frames) && a.frames >= 4,
        "--frames must be an integer of at least 4",
    );
    const modes = a.modes.split(",");
    requireValue(
        modes.length &&
            new Set(modes).size === modes.length &&
            modes.every((m) => ["reference", "fast"].includes(m)),
        "Choose reference, fast, or reference,fast",
    );
    const cfg = await catalog(),
        projects = selectProjects(cfg, a.projects),
        work = path.resolve(a["work-dir"]);
    const { env: clean, removed } = cleanEnvironment(1);
    const binaries = {};
    for (const label of ["base", "new"]) {
        if (!a[label]) continue;
        const binary = path.resolve(a[label]);
        binaries[label] = {
            binary,
            sha256: await fileSha(binary),
            revision: a[`${label}-revision`],
            version: (await exec([binary, "--version"], { env: clean, timeout: 30 })).stdout.trim(),
        };
    }
    const inputs = [];
    for (const p of projects) {
        const { cwd, config } = projectPath(p, work);
        requireValue(
            fs.existsSync(config),
            `${p.name}: missing ${config}; run vp run bench prepare --projects ${p.name}`,
        );
        const revision = (await exec(["git", "rev-parse", "HEAD"], { cwd })).stdout.trim();
        requireValue(
            revision === (p.suite_dir ? cfg.suite.commit : p.commit),
            `${p.name}: checkout does not match the benchmark catalog`,
        );
        const setupRoot = p.suite_dir ? path.join(work, "suite") : cwd;
        const marker = await readJson(path.join(setupRoot, MARKER));
        const overlay = p.overlay ? await overlaySha(p) : undefined;
        requireValue(
            marker.commit === revision &&
                marker.repo === (p.suite_dir ? cfg.suite.repo : p.repo) &&
                marker.install === (p.install ?? null) &&
                marker.overlay === overlay,
            `${p.name}: preparation is incomplete or stale; run vp run bench prepare --projects ${p.name}`,
        );
        inputs.push({ ...p, cwd, config, revision, overlay });
    }
    const out = path.resolve(a.out);
    fs.mkdirSync(path.dirname(out), { recursive: true });
    fs.mkdirSync(out); // Refuse to overwrite profiles from an earlier run.
    const result = {
        schemaVersion: 1,
        status: "running",
        startedAt: new Date().toISOString(),
        machine: machine(),
        binaries,
        projects: inputs,
        modes,
        repetitions: a.reps,
        frames: a.frames,
        clearedEnvironmentVariables: removed,
        coverage: "DHAT global allocator only; direct arena mappings excluded. Heap peak is not process RSS.",
        cells: [],
    };
    const save = () => writeJson(path.join(out, "summary.json"), result);
    await save();
    try {
        for (const p of inputs)
            for (const mode of modes) {
                const cell = { project: p.name, mode, samples: [], summary: {} };
                result.cells.push(cell);
                let expected;
                for (let rep = 0; rep < a.reps; rep++) {
                    const order = Object.keys(binaries);
                    if (rep % 2) order.reverse();
                    for (const label of order) {
                        const prefix = `${p.name}-${mode}-${label}-${rep + 1}`;
                        const profileFile = path.join(out, `${prefix}.dhat.json`);
                        const command = [
                            binaries[label].binary,
                            "-p",
                            p.config,
                            ...TIMED_FLAGS,
                            "--singleThreaded",
                        ];
                        const env = {
                            ...clean,
                            TSRS_FAST: mode === "fast" ? "1" : "0",
                            TSRS_DHAT_FILE: profileFile,
                            TSRS_DHAT_FRAMES: String(a.frames),
                        };
                        console.log(`${prefix}: profiling`);
                        const run = await exec(command, {
                            cwd: p.cwd,
                            env,
                            timeout: a.timeout,
                            // Save compiler output even on a crash or timeout before rejecting the run.
                            accept: Array.from({ length: 256 }, (_, i) => i),
                        });
                        fs.writeFileSync(path.join(out, `${prefix}.stdout`), run.stdout);
                        fs.writeFileSync(path.join(out, `${prefix}.stderr`), run.stderr);
                        requireValue(
                            [0, 1, 2].includes(run.code),
                            `${prefix}: compiler failed (exit ${run.code})`,
                        );
                        requireValue(
                            !CRASH_RE.test(run.stdout + "\n" + run.stderr),
                            `${prefix}: compiler crashed`,
                        );
                        requireValue(
                            fs.existsSync(profileFile),
                            `${prefix}: no profile; build tsrs_cli with --features dhat-heap`,
                        );
                        const output = {
                            exitCode: run.code,
                            diagnostics: diagnostics(run.stdout + "\n" + run.stderr),
                        };
                        expected ??= JSON.stringify(output);
                        requireValue(
                            JSON.stringify(output) === expected,
                            `${prefix}: ordered diagnostics or exit status differ`,
                        );
                        const { sites, ...stats } = summarize(await readJson(profileFile));
                        await writeJson(path.join(out, `${prefix}.sites.json`), sites);
                        cell.samples.push({
                            label,
                            repetition: rep + 1,
                            command,
                            exitCode: run.code,
                            diagnosticSha256: sha(JSON.stringify(output.diagnostics)),
                            profile: profileFile,
                            ...stats,
                        });
                        console.log(
                            `${prefix}: ${stats.totalBlocks} blocks, ${stats.totalBytes} bytes, heap peak ${stats.peakBytes} bytes`,
                        );
                        await save();
                    }
                }
                for (const label of Object.keys(binaries)) {
                    const samples = cell.samples.filter((s) => s.label === label);
                    cell.summary[label] = Object.fromEntries(
                        Object.keys(fields).map((k) => [
                            k,
                            {
                                median: median(samples.map((s) => s[k])),
                                min: Math.min(...samples.map((s) => s[k])),
                                max: Math.max(...samples.map((s) => s[k])),
                            },
                        ]),
                    );
                }
                if (binaries.base)
                    cell.deltaPercent = Object.fromEntries(
                        Object.keys(fields).map((k) => {
                            const base = cell.summary.base[k].median,
                                next = cell.summary.new[k].median;
                            return [k, base ? (next / base - 1) * 100 : null];
                        }),
                    );
                await save();
            }
        result.status = "complete";
    } catch (error) {
        result.status = "failed";
        result.error = error.message;
        throw error;
    } finally {
        result.finishedAt = new Date().toISOString();
        await save();
        console.log(`DHAT results: ${out}`);
    }
});
