import { spawn } from "node:child_process";
import fs from "node:fs";
import module from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
// npm/build.mjs copies platform.js from npm/tsrs/lib, so both packages pick the same platform package suffix.
import { getPlatformPackageName } from "./platform.js";

// The binary ships in `<this package>-<os>-<cpu>[-<libc>]`, an optional dependency per platform like tsrs's.
function getExePath() {
    const override = process.env.TSRS_LINT_BINARY;
    if (override) {
        if (!fs.existsSync(override)) {
            throw new Error(`TSRS_LINT_BINARY is set to ${override}, which does not exist.`);
        }
        return override;
    }

    const __dirname = path.dirname(fileURLToPath(import.meta.url));
    const pkg = JSON.parse(fs.readFileSync(path.join(__dirname, "..", "package.json"), "utf8"));
    const platform = getPlatformPackageName().replace(/^@tsrs\//, "");
    const platformPackageName = `${pkg.name}-${platform}`;
    const exeName = process.platform === "win32" ? "tsrs-lint.exe" : "tsrs-lint";

    let packageDir;
    try {
        const require = module.createRequire(import.meta.url);
        packageDir = path.dirname(require.resolve(`${platformPackageName}/package.json`));
    }
    catch {
        const supported = Object.keys(pkg.optionalDependencies ?? {});
        throw new Error(
            [
                `Cannot find the optional dependency ${platformPackageName}, which contains the tsrs-lint binary for ${platform}.`,
                supported.includes(platformPackageName)
                    ? `It is listed in ${pkg.name}'s optionalDependencies but was not installed. Common causes: installing with --no-optional / --omit=optional, ` +
                        `a lockfile or node_modules created on another platform, or pnpm \`supportedArchitectures\` excluding this platform. Reinstall to fix.`
                    : `This platform is not supported. Supported: ${supported.map(n => n.slice(pkg.name.length + 1)).join(", ")}.`,
                `To use a locally built binary instead, set TSRS_LINT_BINARY=/path/to/${exeName}.`,
            ].join("\n"),
        );
    }

    let exe = path.join(packageDir, exeName);
    if (process.platform === "win32" && exe.length >= 248) {
        exe = "\\\\?\\" + exe;
    }
    if (!fs.existsSync(exe)) {
        throw new Error(`${platformPackageName} is installed at ${packageDir} but has no ${exeName}.`);
    }
    return exe;
}

// Node makes piped stdio non-blocking. Restore blocking mode before Rust uses it: oxlint pipes the request in and
// reads binary frames back.
if (!process.stdout.isTTY) process.stdout._handle?.setBlocking?.(true);
if (!process.stdin.isTTY) process.stdin._handle?.setBlocking?.(true);
if (!process.stderr.isTTY) process.stderr._handle?.setBlocking?.(true);

let exe;
try {
    exe = getExePath();
}
catch (e) {
    console.error(`tsrs-lint: ${e.message}`);
    process.exit(1);
}
const args = process.argv.slice(2);

// Node >= 22.15 / 23.11: replace this process with the binary, so exit codes and signals need no forwarding.
if (process.platform !== "win32" && typeof process.execve === "function") {
    try {
        process.execve(exe, [exe, ...args]);
    }
    catch {
        // Not permitted here (e.g. worker or sandbox); fall back to a child process.
    }
}

const child = spawn(exe, args, { stdio: "inherit" });

const forwarded = ["SIGINT", "SIGTERM", "SIGHUP", "SIGQUIT", "SIGBREAK"];
const handlers = new Map();
for (const signal of forwarded) {
    const handler = () => child.kill(signal);
    try {
        process.on(signal, handler);
        handlers.set(signal, handler);
    }
    catch {
        // Signal not supported on this platform.
    }
}

child.on("error", e => {
    console.error(`tsrs-lint: failed to run ${exe}: ${e.message}`);
    process.exit(1);
});

child.on("exit", (code, signal) => {
    for (const [s, handler] of handlers) process.off(s, handler);
    if (signal) {
        // Die the same way the binary did, so callers see the signal rather than an exit code.
        process.kill(process.pid, signal);
        return;
    }
    process.exit(code ?? 1);
});
