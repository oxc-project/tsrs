import fs from "node:fs";
import path from "node:path";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";

const root = path.join(process.env.RUNNER_TEMP, "pgo");
const bin = path.join(root, "target/aarch64-pc-windows-msvc/release");
const logs = path.join(root, "probe-logs");
const deadline = Date.now() + 30 * 60_000;
const jobs = 2;
const recycle = 200;
fs.mkdirSync(logs, { recursive: true });
const results = [];
function record(entry) {
    results.push(entry);
    console.log(JSON.stringify(entry));
    fs.writeFileSync(path.join(root, "probe-report.json"), JSON.stringify(results, null, 2));
}
function describe(file) {
    const contents = fs.readFileSync(file);
    return { file, bytes: contents.length, sha256: createHash("sha256").update(contents).digest("hex") };
}
function start(name, command, args, env = {}) {
    const started = Date.now();
    const entry = { name, command, args, status: null, signal: null, log: path.join(logs, `${name}.txt`) };
    const fd = fs.openSync(entry.log, "w");
    let child;
    const finished = new Promise(resolve => {
        try {
            child = spawn(command, args, { env: { ...process.env, ...env }, stdio: ["ignore", fd, fd] });
            entry.pid = child.pid;
            child.on("error", error => { entry.error = error.message; });
            child.once("close", (status, signal) => {
                Object.assign(entry, { status, signal, elapsedMs: Date.now() - started });
                resolve(true);
            });
        } catch (error) {
            entry.error = error.message;
            resolve(true);
        }
    });
    fs.closeSync(fd);
    return { child, finished, entry };
}
async function waitFor(finished, timeoutMs) {
    let timer;
    try {
        return await Promise.race([finished, new Promise(resolve => { timer = setTimeout(() => resolve(false), timeoutMs); })]);
    } finally {
        clearTimeout(timer);
    }
}
function tail(file) {
    const fd = fs.openSync(file, "r");
    try {
        const bytes = fs.fstatSync(fd).size;
        const buffer = Buffer.alloc(Math.min(bytes, 6000));
        const read = fs.readSync(fd, buffer, 0, buffer.length, Math.max(0, bytes - buffer.length));
        console.log(buffer.toString("utf8", 0, read));
    } finally {
        fs.closeSync(fd);
    }
}
async function run(name, command, args, env = {}, timeoutMs = 60_000) {
    const remaining = deadline - Date.now();
    if (remaining <= 0) throw new Error("30 minute comparison deadline exceeded");
    const { child, finished, entry } = start(name, command, args, env);
    entry.timeoutMs = Math.min(timeoutMs, remaining);
    entry.timedOut = !await waitFor(finished, entry.timeoutMs);
    if (entry.timedOut) {
        record({ ...entry, phase: "deadline" });
        const cleanup = start(`${name}-taskkill`, "taskkill.exe", ["/PID", String(child.pid), "/T", "/F"]);
        cleanup.entry.timeoutMs = 15_000;
        cleanup.entry.timedOut = !await waitFor(cleanup.finished, cleanup.entry.timeoutMs);
        if (cleanup.entry.timedOut) {
            try { cleanup.entry.killRequested = cleanup.child.kill("SIGKILL"); }
            catch (error) { cleanup.entry.killError = error.message; }
        }
        const cleanupDeadline = Date.now() + 5000;
        cleanup.entry.closed = await waitFor(cleanup.finished, 5000);
        entry.closed = await waitFor(finished, Math.max(1, cleanupDeadline - Date.now()));
        cleanup.child?.unref();
        child.unref();
        entry.cleanupEstablished = !cleanup.entry.timedOut && !cleanup.entry.error
            && cleanup.entry.status === 0 && entry.closed;
        record(cleanup.entry);
        record(entry);
        tail(cleanup.entry.log);
        if (!entry.cleanupEstablished) throw new Error(`${name}: process-tree cleanup could not be established; stopping comparison`);
    } else {
        record(entry);
    }
    tail(entry.log);
    return entry.status === 0 && !entry.error && !entry.timedOut;
}
async function requireIdle(name) {
    // An exited coordinator can leave workers whose ancestry taskkill can no longer follow.
    const script = `
$ErrorActionPreference = "Stop"
try {
    $remaining = @(Get-CimInstance Win32_Process -Filter "Name = 'tsrs.exe' OR Name = 'tsrs-test.exe' OR Name = 'node.exe' OR Name = 'pwsh.exe'" | Where-Object {
        $_.ProcessId -ne $PID -and (
            -not $_.ExecutablePath -or -not $_.CommandLine -or -not $_.CreationDate -or
            [IO.Path]::GetDirectoryName($_.ExecutablePath) -eq $env:PGO_PROBE_BIN -or
            $_.CommandLine.Contains($env:PGO_PROBE_ROOT, [StringComparison]::OrdinalIgnoreCase)
        )
    } | Select-Object ProcessId, ParentProcessId, CreationDate, ExecutablePath, CommandLine)
    ConvertTo-Json -InputObject $remaining -Compress
    if ($remaining.Count -ne 0) { exit 1 }
} catch { Write-Error $_; exit 1 }
`;
    if (!await run(`idle-${name}`, "pwsh", ["-NoProfile", "-Command", script], { PGO_PROBE_BIN: bin, PGO_PROBE_ROOT: root }, 20_000)) {
        throw new Error(`${name}: cannot establish that training processes are gone; stopping comparison`);
    }
}
function expectedWorkers(tests) {
    let workers = 0;
    for (let queued = tests; queued > 0; workers++) {
        queued -= Math.min(queued, Math.max(1, Math.min(Math.ceil(queued / Math.max(jobs, 1)), Math.max(recycle, 1))));
    }
    return workers;
}
async function main() {
    record({ name: "binaries", head: process.env.GITHUB_SHA, files: ["tsrs.exe", "tsrs-test.exe"].map(file => describe(path.join(bin, file))) });
    const trainer = fs.readFileSync(".github/scripts/pgo-train-windows.ps1", "utf8");
    const runner = fs.readFileSync("crates/tsrs_testrunner/src/main.rs", "utf8");
    const pool = fs.readFileSync("crates/tsrs_testrunner/src/pool.rs", "utf8");
    const suiteCommands = trainer.match(/^& .*tsrs-test\.exe.*$/gm) ?? [];
    if (suiteCommands.length !== 1 || suiteCommands[0].trim() !== `& (Join-Path $BinDir "tsrs-test.exe") run --suite all --baselines all --jobs ${jobs} --timeout 120`
        || !runner.includes(`let recycle = args.num("--recycle").unwrap_or(${recycle});`)
        || !pool.replace(/\s+/g, " ").includes("let next_batch = |queue: &mut VecDeque<usize>, jobs: usize, recycle: usize| -> Vec<usize> { let per_worker = queue.len().div_ceil(jobs.max(1)).clamp(1, recycle.max(1)); let n = per_worker.min(queue.len()); queue.drain(..n).collect() };")) {
        throw new Error("trainer jobs, runner recycle or pool batching changed; review the expected worker count");
    }
    if (trainer.split("suite-%m.profraw").length !== 2) throw new Error("expected one suite profile pattern");
    record({ name: "worker-count-contract", jobs, recycle, limitation: "Complete readable profiles do not prove clean worker exit after profile writing." });
    await requireIdle("baseline");
    let ok = await run("profdata-version", process.env.PROFDATA, ["--version"]);
    for (const pattern of ["m", "p"]) {
        const dir = path.join(root, `probe-${pattern}`);
        fs.mkdirSync(dir, { recursive: true });
        if (fs.readdirSync(dir).length !== 0) throw new Error(`expected fresh profiles in ${dir}`);
        for (let attempt = 1; attempt <= 2; attempt++) {
            ok = await run(`help-${pattern}-${attempt}`, path.join(bin, "tsrs-test.exe"), ["--help"], {
                LLVM_PROFILE_FILE: path.join(dir, `probe-%${pattern}.profraw`),
            }) && ok;
            await requireIdle(`help-${pattern}-${attempt}`);
        }
        const files = fs.readdirSync(dir).filter(file => file.endsWith(".profraw"));
        const profiles = files.map(file => ({ file, bytes: fs.statSync(path.join(dir, file)).size }));
        const valid = profiles.length === (pattern === "p" ? 2 : 1) && profiles.every(profile => profile.bytes > 0);
        const entry = { name: `profiles-${pattern}`, status: valid ? 0 : 1, profiles };
        record(entry);
        ok = valid && ok;
        for (const [index, file] of files.entries()) {
            ok = await run(`read-${pattern}-${index}`, process.env.PROFDATA, ["show", path.join(dir, file)]) && ok;
        }
    }
    const testResults = path.join(root, "test-results");
    for (let attempt = 1; attempt <= 2; attempt++) {
        for (const pattern of ["m", "p"]) {
            const name = `${pattern}-${attempt}`;
            const dir = path.join(root, "training", name);
            const raw = path.join(dir, "raw");
            fs.mkdirSync(dir, { recursive: true });
            if (fs.readdirSync(dir).length !== 0) throw new Error(`expected fresh training directory ${dir}`);
            fs.mkdirSync(raw);
            // Keep every workload and gate; only the conformance profile filename varies.
            const script = path.join(dir, "train.ps1");
            fs.writeFileSync(script, trainer.replace("suite-%m.profraw", `suite-%${pattern}.profraw`));
            fs.rmSync(testResults, { recursive: true, force: true });
            const trainingOk = await run(`training-${name}`, "pwsh", ["-NoProfile", "-File", script, "-BinDir", bin, "-RawDir", raw], {}, 300_000);
            ok = trainingOk && ok;
            await requireIdle(`training-${name}`);
            if (fs.existsSync(testResults)) fs.cpSync(testResults, path.join(dir, "test-results"), { recursive: true });
            const profiles = fs.readdirSync(raw).filter(file => file.endsWith(".profraw")).sort().map(file => describe(path.join(raw, file)));
            const count = prefix => profiles.filter(profile => path.basename(profile.file).startsWith(`${prefix}-`)).length;
            let tests = null;
            let summaryError;
            try {
                const summary = JSON.parse(fs.readFileSync(path.join(testResults, "summary.json"), "utf8"));
                if (!Array.isArray(summary.tests)) throw new Error("summary.tests is not an array");
                tests = summary.tests.length;
            } catch (error) { summaryError = error.message; }
            const workers = tests === null ? null : expectedWorkers(tests);
            const expected = { suite: pattern === "m" ? 1 : workers === null ? null : workers + 1, cli: 1, lint: 1 };
            const observed = { suite: count("suite"), cli: count("cli"), lint: count("lint") };
            const valid = trainingOk && tests >= 10000 && profiles.every(profile => profile.bytes > 0)
                && Object.keys(expected).every(prefix => observed[prefix] === expected[prefix])
                && profiles.length === expected.suite + expected.cli + expected.lint;
            record({ name: `profiles-${name}`, status: valid ? 0 : 1, tests, workers, expected, observed, summaryError, profiles });
            ok = valid && ok;
            for (const [index, profile] of profiles.entries()) {
                ok = await run(`read-${name}-${index}`, process.env.PROFDATA, ["show", profile.file]) && ok;
            }
            if (profiles.length > 0) {
                const list = path.join(dir, "profiles.txt");
                fs.writeFileSync(list, `${profiles.map(profile => profile.file).join("\n")}\n`);
                const merged = path.join(dir, "merged.profdata");
                const mergedOk = await run(`merge-${name}`, process.env.PROFDATA, ["merge", "--failure-mode=any", "--input-files", list, "-o", merged]);
                ok = mergedOk && ok;
                if (mergedOk) ok = await run(`read-merged-${name}`, process.env.PROFDATA, ["show", merged]) && ok;
            }
        }
    }
    return ok;
}
try {
    process.exitCode = await main() ? 0 : 1;
} catch (error) {
    record({ name: "comparison-aborted", status: 1, error: error.stack });
    process.exitCode = 1;
}
