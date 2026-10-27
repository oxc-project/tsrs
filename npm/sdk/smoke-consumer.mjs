#!/usr/bin/env node
// Installs the packed tarballs from `node npm/build.mjs ... --pack` into a fresh consumer project outside the repo
// and exercises the published package: the CLI, the root version export, a nodenext typecheck of the API
// declarations (with the packaged binary), and real sync + async compiles through the JS API.
//
//   node npm/sdk/smoke-consumer.mjs [--dist npm/dist] [--node <node binary>]... [--keep] [--noembed] [--cli-only]
//
// --node (repeatable) runs the consumer under each given Node binary instead of the current one, e.g. to check the
// package's `engines` minimum.
//
// Nothing is fetched from the registry: the main package has no runtime dependencies and its platform package is
// installed from the same dist directory. The binary inside the platform package is whatever --binary packed (normally the
// tsrs release build; any TypeScript 7 `--api`-compatible binary, e.g. a pinned tsgo, also works for comparison).

import { execFileSync, spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const distArg = args.indexOf("--dist");
const dist = path.resolve(distArg >= 0 ? args[distArg + 1] : path.join(here, "..", "dist"));
const keep = args.includes("--keep");
const cliOnly = args.includes("--cli-only");
const nodes = args.flatMap((a, i) => a === "--node" ? [path.resolve(args[i + 1])] : []);
if (nodes.length === 0) nodes.push(process.execPath);

const manifest = JSON.parse(fs.readFileSync(path.join(dist, "packages.json"), "utf8"));
const tarballs = manifest.packages.map(p => p.tarball);
if (tarballs.some(t => !t)) throw new Error("packages.json has no tarballs; run npm/build.mjs with --pack");

const failures = [];
const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-consumer-"));
const run = (cmd, argv, opts = {}) => {
    console.log(`$ ${[cmd, ...argv].join(" ")}`);
    return execFileSync(cmd, argv, { cwd: dir, stdio: ["ignore", "pipe", "inherit"], encoding: "utf8", ...opts });
};
try {
    fs.writeFileSync(path.join(dir, "package.json"), JSON.stringify({ name: "tsrs-consumer", private: true, type: "module" }));
    run("pnpm", ["add", "--offline", ...tarballs], { shell: process.platform === "win32" });

    const pkg = JSON.parse(fs.readFileSync(path.join(dir, "node_modules/tsrs/package.json"), "utf8"));
    for (const sub of Object.keys(pkg.exports)) {
        if (!sub.startsWith("./") || sub === "./package.json" || sub.startsWith("./schemas/")) continue;
        const target = pkg.exports[sub].default;
        if (!fs.existsSync(path.join(dir, "node_modules/tsrs", target))) throw new Error(`export ${sub} -> ${target} is not in the tarball`);
        if (!fs.existsSync(path.join(dir, "node_modules/tsrs", target.replace(/\.js$/, ".d.ts")))) throw new Error(`export ${sub} has no declarations`);
    }

    const { default: getExePath } = await import(pathToFileURL(path.join(dir, "node_modules/tsrs/lib/getExePath.js")));
    const { getPlatformPackageName } = await import(pathToFileURL(path.join(dir, "node_modules/tsrs/lib/platform.js")));
    const exe = getExePath();
    if (args.includes("--noembed")) {
        const binary = fs.readFileSync(exe);
        for (const name of ["lib.es5.d.ts", "lib.dom.d.ts"]) {
            const typings = fs.readFileSync(path.join(path.dirname(exe), name));
            if (binary.includes(typings)) throw new Error(`${name} is still embedded in the npm binary`);
        }
        console.log("noembed ok: standard libraries are on disk and absent from the binary");
    }

    const bin = path.join(dir, "node_modules", ".bin", process.platform === "win32" ? "tsrs.cmd" : "tsrs");
    const cliVersion = run(bin, ["--version"], { shell: process.platform === "win32" }).trim();
    console.log(cliVersion);

    // CLI/API keep using tsrs; Windows also ships the executable name VS Code discovers.
    const nativeExe = path.join(dir, "node_modules", getPlatformPackageName(), process.platform === "win32" ? "tsrs.exe" : "tsrs");
    if (fs.realpathSync(nativeExe) !== fs.realpathSync(exe)) throw new Error("CLI/API must resolve the tsrs executable");
    const sdk = process.platform === "win32" ? path.join(path.dirname(nativeExe), "tsgo.exe") : path.join(dir, "node_modules/tsrs/tsdk/tsgo");
    if (process.platform === "win32" && !fs.readFileSync(sdk).equals(fs.readFileSync(nativeExe))) {
        throw new Error("Windows SDK compatibility executable differs from tsrs.exe");
    }
    if (run(nativeExe, ["--version"]).trim() !== cliVersion) throw new Error("Native executable version differs from the CLI");
    if (run(sdk, ["--version"]).trim() !== cliVersion) throw new Error("SDK version differs from the CLI");
    fs.writeFileSync(path.join(dir, "bad.ts"), 'const x: number = "no";\n');
    const sdkBad = spawnSync(sdk, ["--noEmit", "--ignoreConfig", "bad.ts"], { cwd: dir, encoding: "utf8" });
    if (sdkBad.status !== 2 || !sdkBad.stdout.includes("TS2322") || sdkBad.stdout.includes("TS2318")) {
        throw new Error(`SDK: expected exit 2 with TS2322 and resolved default libraries, got ${sdkBad.status}\n${sdkBad.stdout}${sdkBad.stderr}`);
    }
    const nativeExit = await smokeSdkLsp(exe);
    const sdkExit = await smokeSdkLsp(sdk);
    if (JSON.stringify(sdkExit) !== JSON.stringify(nativeExit)) {
        throw new Error(`SDK LSP exit differs from the native binary: ${JSON.stringify({ nativeExit, sdkExit })}`);
    }
    console.log("SDK ok: version, diagnostic exit status, default libraries and LSP initialize/shutdown");

    fs.copyFileSync(path.join(here, "smoke", "typed.mts"), path.join(dir, "typed.mts"));
    fs.writeFileSync(path.join(dir, "tsconfig.json"), JSON.stringify({
        // esnext.disposable: the API classes implement Symbol.dispose / Symbol.asyncDispose, as upstream's do.
        compilerOptions: { module: "nodenext", target: "es2022", lib: ["es2022", "esnext.disposable"], strict: true, noEmit: true, types: [], skipLibCheck: false },
        files: ["typed.mts"],
    }));
    run(bin, ["-p", "tsconfig.json"], { shell: process.platform === "win32" });
    console.log("typecheck ok: API declarations from a nodenext consumer");

    if (!cliOnly) fs.copyFileSync(path.join(here, "smoke", "consumer.mjs"), path.join(dir, "consumer.mjs"));
    for (const node of nodes) {
        const version = run(node, ["--version"]).trim();
        // The CLI launcher under this Node: --version, and a type error must exit 2 (the binary's own code).
        run(node, [path.join(dir, "node_modules/tsrs/bin/tsrs"), "--version"]);
        fs.writeFileSync(path.join(dir, "bad.ts"), 'const x: number = "no";\n');
        const bad = spawnSync(node, [path.join(dir, "node_modules/tsrs/bin/tsrs"), "--noEmit", "--ignoreConfig", "bad.ts"], { cwd: dir, encoding: "utf8" });
        if (bad.status !== 2 || !bad.stdout.includes("TS2322")) throw new Error(`launcher under node ${version}: expected exit 2 with TS2322, got ${bad.status}\n${bad.stdout}${bad.stderr}`);
        if (cliOnly) continue;
        // The consumer runs every step and reports each; a failing step does not stop the other Node versions.
        const consumer = spawnSync(node, ["consumer.mjs", path.join(dir, "project")], { cwd: dir, encoding: "utf8", timeout: 300_000 });
        for (const line of `${consumer.stdout}`.split("\n").filter(Boolean)) console.log(`[node ${version}] ${line}`);
        if (consumer.stderr) process.stderr.write(consumer.stderr.split("\n").map(l => l && `[node ${version}] stderr: ${l}`).join("\n"));
        if (consumer.status !== 0 || !consumer.stdout.includes("smoke ok")) {
            failures.push(`node ${version}: consumer exit ${consumer.status}${consumer.signal ? ` (${consumer.signal})` : ""}`);
        }
    }
    if (failures.length) {
        process.exitCode = 1;
        console.log(`FAILED: ${failures.join("; ")}`);
    }
}
finally {
    if (keep) console.log(`kept ${dir}`);
    else fs.rmSync(dir, { recursive: true, force: true });
}

async function smokeSdkLsp(sdk) {
    const { default: rpc } = await import(pathToFileURL(path.join(dir, "node_modules/tsrs/vendor/vscode-jsonrpc/lib/node/main.js")));
    const child = spawn(sdk, ["--lsp", "--stdio"], { cwd: dir, stdio: "pipe" });
    let stderr = "";
    child.stderr.on("data", chunk => { stderr += chunk; });
    const closed = new Promise((resolve, reject) => {
        child.once("error", reject);
        child.once("close", (code, signal) => resolve({ code, signal }));
    });
    // A spawn error can precede the initialize request rejecting; cleanup awaits it below.
    closed.catch(() => {});
    const connection = rpc.createMessageConnection(new rpc.StreamMessageReader(child.stdout), new rpc.StreamMessageWriter(child.stdin));
    let timer;
    try {
        connection.listen();
        return await Promise.race([
            (async () => {
                const result = await connection.sendRequest("initialize", {
                    processId: process.pid,
                    rootUri: pathToFileURL(dir).href,
                    capabilities: {},
                });
                if (!result.capabilities) throw new Error("SDK LSP initialize returned no capabilities");
                await connection.sendNotification("initialized", {});
                await connection.sendRequest("shutdown");
                await connection.sendNotification("exit");
                const { code, signal } = await closed;
                return { code, signal, stderr };
            })(),
            new Promise((_, reject) => {
                timer = setTimeout(() => reject(new Error(`SDK LSP timed out: ${stderr}`)), 30_000);
            }),
        ]);
    }
    finally {
        clearTimeout(timer);
        connection.dispose();
        if (child.exitCode === null && child.signalCode === null) child.kill();
        await closed;
    }
}
