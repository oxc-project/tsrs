#!/usr/bin/env node
// Real-world declaration bundling benchmark: tsrs_dts against rolldown + rolldown-plugin-dts. See docs/DTS.md#benchmark.
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { measure } from "../measure.mjs";
import { ROOT, args, json, main, median, requireValue, run, shellJoin, write, writeJSON } from "../../tools/lib/cli.mjs";

const HERE = path.join(ROOT, "bench/dts");
const CATALOG = json(path.join(HERE, "projects.json"));
const usage = `Declaration bundler benchmark: tsrs_dts against rolldown + rolldown-plugin-dts.

  node bench/dts/dts.mjs prepare [--projects a,b] [--work-dir dir]
  node bench/dts/dts.mjs run [--projects a,b] [--tools a,b] [--runs N] [--warmup N] [--tsrs binary]
                             [--work-dir dir] [--out-dir dir]
  node bench/dts/dts.mjs report <results.json>

Tools: `;

// Every tool reads the same tsconfig.bench.json and entries. tsrs always checks the source and the emitted
// declarations; rolldown-plugin-dts never checks (its tsc generator only emits, its tsgo generator passes
// --noCheck), so the "check &&" rows add the type check a release build would run anyway.
const TOOLS = {
    tsrs: { label: "tsrs_dts", checks: true },
    "tsrs-reuse": { label: "tsrs_dts (reuse diagnostics)", checks: true },
    "rolldown-tsc": { label: "rolldown + plugin-dts (tsc 6)", checks: false, generator: "tsc" },
    "rolldown-tsgo": { label: "rolldown + plugin-dts (tsgo 7)", checks: false, generator: "tsgo" },
    "check-tsc+rolldown-tsc": { label: "tsc 6 --noEmit && rolldown (tsc)", checks: true, generator: "tsc" },
    "check-tsgo+rolldown-tsgo": { label: "tsgo 7 --noEmit && rolldown (tsgo)", checks: true, generator: "tsgo" },
};

const list = (value) => value.split(",").map((item) => item.trim()).filter(Boolean);
function selectProjects(names) {
    const all = CATALOG.projects;
    if (!names) return all;
    return list(names).map((name) => {
        const project = all.find((item) => item.name === name);
        requireValue(project, `unknown project ${name}; known: ${all.map((item) => item.name).join(", ")}`);
        return project;
    });
}
const projectDir = (work, project) => path.join(work, "projects", project.name);
const tsconfigPath = (work, project) => path.join(projectDir(work, project), "tsconfig.bench.json");
const entryArgs = (work, project) =>
    Object.entries(project.entries).map(([name, entry]) => `${name}=${path.join(projectDir(work, project), entry)}`);
const sha256 = (file) => crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
const toolsInstalled = (work) => path.join(work, "node_modules");
const typescript7 = async (work) => (await import(path.join(work, "node_modules/typescript7/lib/getExePath.js"))).default();
const typescript6 = (work) => path.join(work, "node_modules/typescript/bin/tsc");

function compilerOptions(project) {
    return { ...CATALOG.compilerOptions, ...project.compilerOptions };
}

async function prepare(options) {
    const work = path.resolve(options["work-dir"]);
    fs.mkdirSync(work, { recursive: true });
    // Reinstall tools only when the pinned lockfile changes.
    const lock = path.join(HERE, "package-lock.json");
    const stamp = path.join(work, ".tools-lock.sha256");
    if (!fs.existsSync(toolsInstalled(work)) || !fs.existsSync(stamp) || fs.readFileSync(stamp, "utf8") !== sha256(lock)) {
        fs.copyFileSync(path.join(HERE, "package.json"), path.join(work, "package.json"));
        fs.copyFileSync(lock, path.join(work, "package-lock.json"));
        run(["npm", "ci", "--ignore-scripts", "--no-audit", "--no-fund"], { cwd: work, inherit: true });
        fs.writeFileSync(stamp, sha256(lock));
    }
    const tsgo = await typescript7(work);
    const failed = [];
    for (const project of selectProjects(options.projects)) {
        const dir = projectDir(work, project);
        const downloads = path.join(work, "downloads");
        fs.mkdirSync(downloads, { recursive: true });
        const packed = JSON.parse(
            run(["npm", "pack", project.package, "--pack-destination", downloads, "--json"], { cwd: work }).stdout,
        )[0];
        requireValue(
            packed.integrity === project.integrity,
            `${project.package}: integrity ${packed.integrity} does not match projects.json`,
        );
        fs.rmSync(dir, { recursive: true, force: true });
        fs.mkdirSync(dir, { recursive: true });
        run(["tar", "-xzf", path.join(downloads, packed.filename), "-C", dir, "--strip-components=1"]);
        writeJSON(tsconfigPath(work, project), {
            compilerOptions: compilerOptions(project),
            files: Object.values(project.entries),
        });
        // Preflight: tsrs_dts rejects any diagnostic, so the inputs must check cleanly.
        const check = run([tsgo, "--noEmit", "-p", tsconfigPath(work, project)], { cwd: dir, check: false });
        if (check.code !== 0) {
            console.error(`${project.name}: preflight type check failed\n${check.stdout}${check.stderr}`);
            failed.push(project.name);
        } else console.log(`prepared ${project.name} (${project.package}) in ${path.relative(ROOT, dir)}`);
    }
    requireValue(failed.length === 0, `preflight failed: ${failed.join(", ")}`);
}

async function commandFor(tool, project, context) {
    const { work, outDir, tsrs, tsgo } = context;
    const config = tsconfigPath(work, project);
    const entries = entryArgs(work, project);
    if (tool === "tsrs" || tool === "tsrs-reuse")
        return [tsrs, "--config", config, "--out-dir", outDir, ...(tool === "tsrs-reuse" ? ["--reuse-diagnostics"] : []), ...entries];
    const { generator } = TOOLS[tool];
    const bundle = [
        process.execPath,
        path.join(HERE, "rolldown.mjs"),
        "--tools",
        work,
        "--tsconfig",
        config,
        "--out-dir",
        outDir,
        "--generator",
        generator,
        ...(generator === "tsgo" ? ["--tsgo", tsgo] : []),
        ...entries,
    ];
    if (!tool.startsWith("check-")) return bundle;
    const check = generator === "tsgo" ? [tsgo] : [process.execPath, typescript6(work)];
    // GNU time and macOS time report the largest peak RSS among the shell's children.
    return ["/bin/sh", "-c", `${shellJoin([...check, "--noEmit", "-p", config])} && ${shellJoin(bundle)}`];
}

// Each entry's exported names and meanings, from the TypeScript 6 API (exports.mjs).
function exportsOf(work, tsconfig, roots) {
    const argv = [process.execPath, path.join(HERE, "exports.mjs"), "--tools", work, "--tsconfig", tsconfig];
    return JSON.parse(run([...argv, ...roots.map(([name, file]) => `${name}=${file}`)]).stdout);
}

// Names a bundle entry lacks or adds relative to its source entry, and names whose value/type/namespace
// meaning changed.
function compareExports(source, bundle) {
    const differences = [];
    for (const [entry, names] of Object.entries(source)) {
        const output = bundle[entry] ?? {};
        for (const [name, meaning] of Object.entries(names))
            if (!(name in output)) differences.push(`${entry}: missing ${name}`);
            else if (output[name] !== meaning) differences.push(`${entry}: ${name} is ${output[name]}, source ${meaning}`);
        for (const name of Object.keys(output)) if (!(name in names)) differences.push(`${entry}: extra ${name}`);
    }
    return differences;
}

// Untimed: type-check the bundle as a consumer would see it and compare its public API with the sources. Each
// output declaration is a root file; the project's libs and types still apply (outputs live under the work
// directory, so @types resolve from its node_modules), and library checking stays on so errors inside the
// bundle surface.
function validate(work, project, outDir, tsgo, sourceExports) {
    const roots = fs
        .readdirSync(outDir, { recursive: true })
        .filter((file) => /\.d\.[mc]?ts$/.test(file))
        .sort();
    // Keep only checking options: emit flags would be invalid with noEmit, and `paths` would map package
    // self-references back to the sources and hide a bundle that imports them.
    const { strict, target, module, moduleResolution, lib, types } = compilerOptions(project);
    const config = path.join(outDir, "tsconfig.consumer.json");
    writeJSON(config, {
        compilerOptions: { strict, target, module, moduleResolution, lib, types, skipLibCheck: false, noEmit: true },
        files: roots,
    });
    const result = run([tsgo, "-p", config], { cwd: outDir, check: false });
    const entries = Object.keys(project.entries).map((name) => [name, path.join(outDir, name)]);
    const differences = compareExports(sourceExports, exportsOf(work, config, entries));
    fs.rmSync(config);
    return {
        ok: result.code === 0,
        errors: (result.stdout.match(/error TS\d+/g) ?? []).length,
        roots: roots.length,
        output: result.code === 0 ? "" : result.stdout.slice(0, 4000),
        exports: { matches: differences.length === 0, count: differences.length, differences: differences.slice(0, 50) },
    };
}

function digest(outDir) {
    const hash = crypto.createHash("sha256");
    for (const file of fs.readdirSync(outDir, { recursive: true }).sort()) {
        const full = path.join(outDir, file);
        if (fs.statSync(full).isFile()) hash.update(file).update("\0").update(fs.readFileSync(full)).update("\0");
    }
    return hash.digest("hex");
}

function versions(work) {
    const version = (name) => json(path.join(work, "node_modules", name, "package.json")).version;
    return {
        rolldown: version("rolldown"),
        "rolldown-plugin-dts": version("rolldown-plugin-dts"),
        typescript: version("typescript"),
        typescript7: version("typescript7"),
        node: process.version,
    };
}

function machine() {
    const cpus = os.cpus();
    return {
        platform: `${process.platform}-${process.arch}`,
        cpu: cpus[0]?.model ?? "unknown",
        logical_cpus: cpus.length,
        memory_gib: Math.round(os.totalmem() / 2 ** 30),
        kernel: os.release(),
    };
}

async function runBenchmark(options) {
    const work = path.resolve(options["work-dir"]);
    requireValue(fs.existsSync(toolsInstalled(work)), `run \`node bench/dts/dts.mjs prepare\` first (no tools in ${work})`);
    const tools = list(options.tools);
    for (const tool of tools) requireValue(TOOLS[tool], `unknown tool ${tool}; known: ${Object.keys(TOOLS).join(", ")}`);
    const tsrs = path.resolve(options.tsrs);
    if (tools.some((tool) => tool.startsWith("tsrs")))
        requireValue(fs.existsSync(tsrs), `${tsrs} is missing; build it with cargo build --release -p tsrs_dts --example bundle`);
    const projects = selectProjects(options.projects);
    for (const project of projects)
        requireValue(fs.existsSync(tsconfigPath(work, project)), `${project.name} is not prepared; run prepare first`);
    const tsgo = await typescript7(work);
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    const outRoot = path.resolve(options["out-dir"] ?? path.join(ROOT, "bench/results", `dts-${stamp}`));
    const scratch = path.join(work, "out");
    fs.mkdirSync(outRoot, { recursive: true });
    const perf = process.platform === "linux" && run(["sh", "-c", "command -v perf"], { check: false }).code === 0;
    const results = {
        kind: "dts-bundle",
        created: new Date().toISOString(),
        commit: run(["git", "rev-parse", "HEAD"], { cwd: ROOT }).stdout.trim(),
        machine: machine(),
        versions: versions(work),
        tsrs: tools.some((tool) => tool.startsWith("tsrs")) ? { path: tsrs, sha256: sha256(tsrs) } : null,
        settings: { runs: options.runs, warmup: options.warmup, instructions: perf },
        tools: Object.fromEntries(tools.map((tool) => [tool, TOOLS[tool]])),
        projects: projects.map((project) => ({ ...project, compilerOptions: compilerOptions(project) })),
        samples: [],
        outputs: {},
    };
    const sourceExports = {};
    for (const project of projects) {
        const entries = Object.entries(project.entries).map(([name, entry]) => [name, path.join(projectDir(work, project), entry)]);
        sourceExports[project.name] = exportsOf(work, tsconfigPath(work, project), entries);
    }
    results.source_exports = Object.fromEntries(
        Object.entries(sourceExports).map(([name, entries]) => [
            name,
            Object.fromEntries(Object.entries(entries).map(([entry, names]) => [entry, Object.keys(names).length])),
        ]),
    );
    const sample = async (project, tool, index, warmup) => {
        const outDir = path.join(scratch, project.name, tool);
        fs.rmSync(outDir, { recursive: true, force: true });
        fs.mkdirSync(outDir, { recursive: true });
        const command = await commandFor(tool, project, { work, outDir, tsrs, tsgo });
        const measured = measure(command, { cwd: projectDir(work, project), timeout: 1800, instructions: perf });
        const status = measured.code === 0 ? "ok" : `exit ${measured.code}`;
        const summaryLine = measured.stdout.trim().split("\n").at(-1) ?? "";
        let summary = null;
        try {
            summary = JSON.parse(summaryLine);
        } catch {}
        const record = {
            project: project.name,
            tool,
            run: index,
            warmup,
            status,
            wall_s: measured.wall_s,
            max_rss_bytes: measured.max_rss_bytes,
            cpu_s: (measured.user ?? 0) + (measured.sys ?? 0),
            instructions: measured.instructions,
            summary,
        };
        if (measured.code !== 0) record.stderr = measured.stderr.slice(-4000);
        console.log(
            `${warmup ? "warmup" : `run ${index + 1}/${options.runs}`} ${project.name} ${tool}: ${status} ` +
                `${(measured.wall_s * 1000).toFixed(0)} ms, ${(measured.max_rss_bytes / 2 ** 20).toFixed(0)} MiB`,
        );
        return { record, outDir };
    };
    // Interleave tools within each run, rotating the order so no tool always runs first.
    const order = (round) => tools.map((_, i) => tools[(i + round) % tools.length]);
    for (let round = 0; round < options.warmup + options.runs; round++) {
        const warmup = round < options.warmup;
        for (const project of projects)
            for (const tool of order(round)) {
                const { record, outDir } = await sample(project, tool, round - options.warmup, warmup);
                if (!warmup) results.samples.push(record);
                const key = `${project.name}/${tool}`;
                if (record.status === "ok" && !results.outputs[key]) {
                    results.outputs[key] = {
                        digest: digest(outDir),
                        consumer: validate(work, project, outDir, tsgo, sourceExports[project.name]),
                    };
                    fs.cpSync(outDir, path.join(outRoot, "outputs", project.name, tool), { recursive: true });
                }
            }
    }
    const file = path.join(outRoot, "results.json");
    writeJSON(file, results);
    write(path.join(outRoot, "report.md"), report(results));
    console.log(file);
}

const mib = (bytes) => bytes / 2 ** 20;
function stats(samples, pick) {
    const values = samples.map(pick).filter((value) => Number.isFinite(value));
    return values.length ? { median: median(values), min: Math.min(...values), max: Math.max(...values) } : null;
}

function report(results) {
    const lines = [];
    const { machine: m, versions: v } = results;
    lines.push(
        `Machine: ${m.cpu}, ${m.logical_cpus} logical CPUs, ${m.memory_gib} GiB, ${m.platform}. ` +
            `rolldown ${v.rolldown}, rolldown-plugin-dts ${v["rolldown-plugin-dts"]}, typescript ${v.typescript}, ` +
            `typescript 7 ${v.typescript7}, Node ${v.node}. tsrs commit ${results.commit.slice(0, 12)}. ` +
            `${results.settings.runs} measured runs after ${results.settings.warmup} warmup(s); medians, with min–max for wall time.`,
        "",
    );
    for (const project of results.projects) {
        const entries = Object.keys(project.entries).length;
        lines.push(`### ${project.name} (${project.package}, ${entries} ${entries === 1 ? "entry" : "entries"})`, "");
        const tools = Object.keys(results.tools);
        const base = stats(
            results.samples.filter((s) => s.project === project.name && s.tool === "tsrs" && s.status === "ok"),
            (s) => s.wall_s,
        );
        const instructions = results.settings.instructions;
        lines.push(
            `| Tool | Type checks | Wall ms | vs tsrs_dts | CPU s | Peak RSS MiB |${instructions ? " Instructions (G) |" : ""} Files | Bytes | Bundle checks | Exports match source |`,
            `| --- | :---: | ---: | ---: | ---: | ---: |${instructions ? " ---: |" : ""} ---: | ---: | :---: | :---: |`,
        );
        for (const tool of tools) {
            const samples = results.samples.filter((s) => s.project === project.name && s.tool === tool);
            const ok = samples.filter((s) => s.status === "ok");
            const output = results.outputs[`${project.name}/${tool}`];
            if (ok.length === 0) {
                const status = samples[0]?.status ?? "not run";
                lines.push(`| ${results.tools[tool].label} | ${results.tools[tool].checks ? "yes" : "no"} | ${status} |  |  |  |${instructions ? "  |" : ""}  |  |  |  |`);
                continue;
            }
            const wall = stats(ok, (s) => s.wall_s * 1000);
            const ratio = base ? `${(wall.median / (base.median * 1000)).toFixed(2)}×` : "";
            const consumer = output ? (output.consumer.ok ? "pass" : `${output.consumer.errors} errors`) : "";
            const exported = output ? (output.consumer.exports.matches ? "yes" : `${output.consumer.exports.count} differ`) : "";
            const failures = samples.length - ok.length;
            lines.push(
                `| ${results.tools[tool].label} | ${results.tools[tool].checks ? "yes" : "no"} | ` +
                    `${wall.median.toFixed(0)} (${wall.min.toFixed(0)}–${wall.max.toFixed(0)})${failures ? `, ${failures} failed` : ""} | ${ratio} | ` +
                    `${stats(ok, (s) => s.cpu_s).median.toFixed(2)} | ${mib(stats(ok, (s) => s.max_rss_bytes).median).toFixed(0)} |` +
                    (instructions ? ` ${((stats(ok, (s) => s.instructions)?.median ?? NaN) / 1e9).toFixed(2)} |` : "") +
                    ` ${ok[0].summary?.files ?? ""} | ${ok[0].summary?.bytes ?? ""} | ${consumer} | ${exported} |`,
            );
        }
        lines.push("");
    }
    return lines.join("\n");
}

main(async () => {
    const [command, ...rest] = process.argv.slice(2);
    if (!command || command === "--help" || command === "-h") {
        console.log(usage + Object.keys(TOOLS).join(", "));
        return;
    }
    if (command === "report") {
        requireValue(rest.length === 1, "usage: report <results.json>");
        process.stdout.write(report(json(rest[0])));
        return;
    }
    const options = args(usage, {
        projects: null,
        tools: Object.keys(TOOLS).join(","),
        runs: 5,
        warmup: 1,
        tsrs: path.join(ROOT, "target/release/examples/bundle"),
        "work-dir": path.join(ROOT, "bench/.work/dts"),
        "out-dir": null,
    }, rest);
    requireValue(options._.length === 0, `unexpected arguments: ${options._.join(" ")}`);
    if (command === "prepare") await prepare(options);
    else if (command === "run") await runBenchmark(options);
    else throw new Error(`unknown command ${command}\n${usage}`);
});
