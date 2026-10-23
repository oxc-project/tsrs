#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { ROOT, args, read, json, write, writeJSON, run, requireValue, pinnedCommit, main } from "../lib/cli.mjs";
const HERE = fileURLToPath(new URL(".", import.meta.url));
export function prepare(reference, work) {
    const commit = pinnedCommit(),
        checkout = path.join(work, "reference"),
        stamp = path.join(checkout, ".tsrs-commit");
    if (!fs.existsSync(stamp) || read(stamp).trim() !== commit) {
        requireValue(
            !fs.existsSync(checkout),
            `${checkout} exists without the matching commit stamp; choose a new --work directory`,
        );
        const temp = fs.mkdtempSync(path.join(work, "extract-")),
            archive = path.join(work, "reference.tar");
        try {
            const fd = fs.openSync(archive, "w");
            try {
                run(["git", "-C", reference, "archive", commit, "tsc"], { stdio: ["ignore", fd, "pipe"] });
            } finally {
                fs.closeSync(fd);
            }
            run(["tar", "-xf", archive, "-C", temp]);
            write(path.join(temp, ".tsrs-commit"), commit + "\n");
            fs.renameSync(temp, checkout);
        } finally {
            fs.rmSync(temp, { recursive: true, force: true });
            fs.rmSync(archive, { force: true });
        }
    }
    const module = path.join(checkout, "tsc"),
        replacements = {},
        edits = json(path.join(HERE, "overlay-edits.json"));
    const overlay = (file, source) => {
        const output = path.join(work, "overlay", path.basename(file));
        write(output, source);
        replacements[path.join(module, file)] = output;
    };
    const sources = {
        client: "internal/testutil/lsptestutil/lspclient.go",
        main: "internal/fourslash/tests/testmain_test.go",
        harness: "internal/fourslash/fourslash.go",
        state: "internal/fourslash/statebaseline.go",
    };
    for (const [key, file] of Object.entries(sources)) {
        let source = read(path.join(module, file));
        for (const [kind, old, value] of edits)
            if (kind === key) {
                requireValue(
                    source.split(old).length === 2,
                    `upstream harness changed: expected one occurrence of ${JSON.stringify(old)}`,
                );
                source = source.replace(old, value);
            }
        if (key === "state")
            source = source
                .replaceAll("*compiler.Program", "*remoteProgram")
                .replaceAll("*project.ConfigFileRegistry", "*remoteConfigFileRegistry")
                .replaceAll("*project.Snapshot", "*remoteSnapshot");
        overlay(file, source);
    }
    for (const file of ["subprocess.go", "subprocess_test.go", "worker.go"])
        overlay(`internal/testutil/lsptestutil/tsrs_${file}`, read(path.join(HERE, file)));
    overlay("internal/fourslash/tsrs_remotestate.go", read(path.join(HERE, "remotestate.go")));
    const env = { ...process.env, GOTOOLCHAIN: "auto", GOWORK: "off" },
        goroot = run(["go", "env", "GOROOT"], { cwd: module, env }).stdout.trim();
    run([path.join(goroot, "bin/gofmt"), "-w", ...Object.values(replacements)]);
    const manifest = path.join(work, "overlay.json");
    writeJSON(manifest, { Replace: replacements });
    return { module, manifest };
}
async function progress(command, cwd, env) {
    const started = performance.now(),
        counts = { pass: 0, fail: 0, skip: 0 },
        output = new Map();
    let running = false;
    const report = () =>
        console.error(
            `${running ? `Fourslash: ${Object.values(counts).reduce((a, b) => a + b)} groups completed (${counts.pass} passed, ${counts.fail} failed, ${counts.skip} skipped)` : "Building Go suite"} [${((performance.now() - started) / 1000).toFixed(1)}s]`,
        );
    report();
    const child = spawn(command[0], command.slice(1), { cwd, env, stdio: ["inherit", "pipe", "inherit"] });
    const done = new Promise((resolve, reject) => {
        child.once("error", reject);
        child.once("close", (code) => resolve(code ?? 1));
    });
    const timer = setInterval(report, process.stderr.isTTY ? 1000 : 5000);
    try {
        for await (const line of createInterface({ input: child.stdout, crlfDelay: Infinity })) {
            let e;
            try {
                e = JSON.parse(line);
            } catch {
                console.log(line);
                continue;
            }
            if (e.Action === "start") running = true;
            const name = e.Test ?? "",
                parent = name.split("/")[0];
            if (e.Output) {
                if (name) {
                    if (!output.has(parent)) output.set(parent, []);
                    output.get(parent).push(e.Output);
                } else process.stdout.write(e.Output);
            }
            if (name && !name.includes("/") && e.Action in counts) {
                counts[e.Action]++;
                if (e.Action === "fail") process.stdout.write((output.get(name) ?? []).join(""));
                output.delete(name);
            }
        }
        const code = await done;
        if (code) for (const parts of output.values()) process.stdout.write(parts.join(""));
        report();
        return code;
    } finally {
        clearInterval(timer);
    }
}
main(async () => {
    const raw = process.argv.slice(2),
        separator = raw.indexOf("--"),
        flags = separator >= 0 ? raw.slice(separator + 1) : [];
    const a = args(
        "Run the pinned Go Fourslash tests against a Rust LSP subprocess. Go flags follow --.",
        { reference: path.join(ROOT, "ts-ref"), work: path.join(ROOT, "target/go-fourslash"), server: null },
        separator >= 0 ? raw.slice(0, separator) : raw,
    );
    const work = path.resolve(a.work);
    fs.mkdirSync(work, { recursive: true });
    console.error("Preparing pinned TypeScript source and Go harness...");
    const { module, manifest } = prepare(path.resolve(a.reference), work);
    const env = { ...process.env, GOTOOLCHAIN: "auto", GOWORK: "off", GOCACHE: path.join(work, "go-cache") };
    let server;
    if (a.server) server = path.resolve(a.server);
    else {
        console.error("Building Rust Fourslash server...");
        env.CARGO_TARGET_DIR = path.join(work, "cargo");
        run(["cargo", "build", "--profile", "dev", "-p", "tsrs_fourslash_server"], { cwd: ROOT, env, inherit: true });
        server = path.join(work, "cargo/debug/fourslash_server");
    }
    requireValue(fs.existsSync(server), `server does not exist: ${server}`);
    env.TSRS_FOURSLASH_SERVER = server;
    console.error("Checking subprocess bridge shutdown...");
    run(["go", "test", `-overlay=${manifest}`, "./internal/testutil/lsptestutil", "-count=1"], {
        cwd: module,
        env,
        stdio: ["inherit", 2, 2],
    });
    const command = [
        "go",
        "test",
        `-overlay=${manifest}`,
        "./internal/fourslash/tests",
        "-count=1",
        "-parallel=4",
        "-timeout=30m",
    ];
    process.exitCode = flags.some((f) =>
        ["json", "v", "test.v", "list", "c"].includes(f.split("=")[0].replace(/^-+/, "")),
    )
        ? run([...command, ...flags], { cwd: module, env, inherit: true, timeout: 1900, check: false }).code
        : await progress([...command, "-json", ...flags], module, env);
});
