import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";

const root = path.join(process.env.RUNNER_TEMP, "pgo");
const bin = path.join(root, "target/aarch64-pc-windows-msvc/release");
const logs = path.join(root, "probe-logs");
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
function run(name, command, args, env = {}) {
    const result = spawnSync(command, args, {
        encoding: "utf8",
        env: { ...process.env, ...env },
        maxBuffer: 32 * 1024 * 1024,
    });
    const output = `${result.stdout ?? ""}\n${result.stderr ?? ""}`;
    fs.writeFileSync(path.join(logs, `${name}.txt`), output);
    const entry = { name, command, args, status: result.status, signal: result.signal, error: result.error?.message };
    record(entry);
    console.log(output.slice(-6000));
    return result.status === 0 && !result.error;
}
record({ name: "binaries", head: process.env.GITHUB_SHA, files: ["tsrs.exe", "tsrs-test.exe"].map(file => describe(path.join(bin, file))) });
let ok = run("profdata-version", process.env.PROFDATA, ["--version"]);
for (const pattern of ["m", "p"]) {
    const dir = path.join(root, `probe-${pattern}`);
    fs.mkdirSync(dir, { recursive: true });
    for (let attempt = 1; attempt <= 2; attempt++) {
        ok = run(`help-${pattern}-${attempt}`, path.join(bin, "tsrs-test.exe"), ["--help"], {
            LLVM_PROFILE_FILE: path.join(dir, `probe-%${pattern}.profraw`),
        }) && ok;
    }
    const files = fs.readdirSync(dir).filter(file => file.endsWith(".profraw"));
    const profiles = files.map(file => ({ file, bytes: fs.statSync(path.join(dir, file)).size }));
    const valid = profiles.length === (pattern === "p" ? 2 : 1) && profiles.every(profile => profile.bytes > 0);
    const entry = { name: `profiles-${pattern}`, status: valid ? 0 : 1, profiles };
    record(entry);
    ok = valid && ok;
    for (const [index, file] of files.entries()) {
        ok = run(`read-${pattern}-${index}`, process.env.PROFDATA, ["show", path.join(dir, file)]) && ok;
    }
}
const trainer = fs.readFileSync(".github/scripts/pgo-train-windows.ps1", "utf8");
if (trainer.split("suite-%m.profraw").length !== 2) throw new Error("expected one suite profile pattern");
const testResults = path.join(root, "test-results");
for (let attempt = 1; attempt <= 2; attempt++) {
    for (const pattern of ["m", "p"]) {
        const name = `${pattern}-${attempt}`;
        const dir = path.join(root, "training", name);
        const raw = path.join(dir, "raw");
        fs.mkdirSync(raw, { recursive: true });
        if (fs.readdirSync(raw).length !== 0) throw new Error(`expected fresh profiles in ${raw}`);
        // Keep every workload and gate; only the conformance profile filename varies.
        const script = path.join(dir, "train.ps1");
        fs.writeFileSync(script, trainer.replace("suite-%m.profraw", `suite-%${pattern}.profraw`));
        fs.rmSync(testResults, { recursive: true, force: true });
        ok = run(`training-${name}`, "pwsh", ["-NoProfile", "-File", script, "-BinDir", bin, "-RawDir", raw]) && ok;
        if (fs.existsSync(testResults)) fs.cpSync(testResults, path.join(dir, "test-results"), { recursive: true });
        const profiles = fs.readdirSync(raw).filter(file => file.endsWith(".profraw")).sort().map(file => describe(path.join(raw, file)));
        const count = prefix => profiles.filter(profile => path.basename(profile.file).startsWith(`${prefix}-`)).length;
        const valid = profiles.every(profile => profile.bytes > 0)
            && (pattern === "m" ? count("suite") === 1 : count("suite") > 1)
            && count("cli") === 1 && count("lint") === 1;
        record({ name: `profiles-${name}`, status: valid ? 0 : 1, profiles });
        ok = valid && ok;
        for (const [index, profile] of profiles.entries()) {
            ok = run(`read-${name}-${index}`, process.env.PROFDATA, ["show", profile.file]) && ok;
        }
        if (profiles.length > 0) {
            const list = path.join(dir, "profiles.txt");
            fs.writeFileSync(list, `${profiles.map(profile => profile.file).join("\n")}\n`);
            const merged = path.join(dir, "merged.profdata");
            const mergedOk = run(`merge-${name}`, process.env.PROFDATA, ["merge", "--failure-mode=any", "--input-files", list, "-o", merged]);
            ok = mergedOk && ok;
            if (mergedOk) ok = run(`read-merged-${name}`, process.env.PROFDATA, ["show", merged]) && ok;
        }
    }
}
process.exitCode = ok ? 0 : 1;
