import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const root = path.join(process.env.RUNNER_TEMP, "pgo");
const bin = path.join(root, "target/aarch64-pc-windows-msvc/release");
const raw = path.join(root, "raw");
const logs = path.join(root, "probe-logs");
fs.mkdirSync(logs, { recursive: true });
const results = [];
function run(name, command, args, env = {}) {
    const result = spawnSync(command, args, {
        encoding: "utf8",
        env: { ...process.env, ...env },
        maxBuffer: 32 * 1024 * 1024,
    });
    const output = `${result.stdout ?? ""}\n${result.stderr ?? ""}`;
    fs.writeFileSync(path.join(logs, `${name}.txt`), output);
    const entry = { name, command, args, status: result.status, signal: result.signal, error: result.error?.message };
    results.push(entry);
    console.log(JSON.stringify(entry));
    console.log(output.slice(-6000));
    fs.writeFileSync(path.join(root, "probe-report.json"), JSON.stringify(results, null, 2));
    return result.status === 0;
}
let ok = true;
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
    results.push(entry);
    console.log(JSON.stringify(entry));
    fs.writeFileSync(path.join(root, "probe-report.json"), JSON.stringify(results, null, 2));
    ok = valid && ok;
    for (const [index, file] of files.entries()) {
        ok = run(`read-${pattern}-${index}`, process.env.PROFDATA, ["show", path.join(dir, file)]) && ok;
    }
}
ok = run("training", "pwsh", ["-NoProfile", "-File", ".github/scripts/pgo-train-windows.ps1", "-BinDir", bin, "-RawDir", raw]) && ok;
const profiles = fs.readdirSync(raw).filter(file => file.endsWith(".profraw") && fs.statSync(path.join(raw, file)).size > 0);
console.log("training profiles", profiles.map(file => ({ file, bytes: fs.statSync(path.join(raw, file)).size })));
if (profiles.length === 0) ok = false;
else ok = run("merge", process.env.PROFDATA, ["merge", "-o", path.join(root, "merged.profdata"), ...profiles.map(file => path.join(raw, file))]) && ok;
process.exitCode = ok ? 0 : 1;
