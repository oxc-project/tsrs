import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, spawnSync } from "node:child_process";
import { parseArgs } from "node:util";

export const ROOT = fileURLToPath(new URL("../../", import.meta.url));
export const read = (file) => fs.readFileSync(file, "utf8");
export const json = (file) => JSON.parse(read(file).replace(/^\uFEFF/, ""));
export function write(file, content) {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, content);
}
export const writeJSON = (file, value) => write(file, JSON.stringify(value, null, 2) + "\n");
export const lines = (text) =>
    text
        .replace(/\r\n/g, "\n")
        .split("\n")
        .filter((line, i, all) => i < all.length - 1 || line !== "");
export const quote = (value) => `'${String(value).replaceAll("'", "'\\''")}'`;
export const shellJoin = (argv) => argv.map(quote).join(" ");
export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
export const median = (values) => {
    const a = values.filter((v) => v !== null && v !== undefined).sort((a, b) => a - b);
    return a.length ? (a[Math.floor((a.length - 1) / 2)] + a[Math.floor(a.length / 2)]) / 2 : null;
};
export function requireValue(ok, message) {
    if (!ok) throw new Error(message);
}
export function args(description, spec = {}, argv = process.argv.slice(2)) {
    const options = { help: { type: "boolean", short: "h" } };
    for (const [key, value] of Object.entries(spec)) {
        options[key] =
            typeof value === "boolean"
                ? { type: "boolean", default: value }
                : Array.isArray(value)
                  ? { type: "string", multiple: true, default: value }
                  : { type: "string", ...(value === null ? {} : { default: String(value) }) };
    }
    // Accept both repeated array options and the old space-separated spelling.
    const normalized = [];
    for (let i = 0; i < argv.length; i++) {
        if (argv[i] === "--") {
            normalized.push(...argv.slice(i));
            break;
        }
        const key = argv[i].startsWith("--") ? argv[i].slice(2) : "";
        if (["files", "conformance", "closed"].includes(key) && Array.isArray(spec[key])) {
            while (i + 1 < argv.length && !argv[i + 1].startsWith("-")) normalized.push(`--${key}=${argv[++i]}`);
        } else normalized.push(argv[i]);
    }
    const { values, positionals } = parseArgs({
        args: normalized,
        options,
        allowPositionals: true,
        allowNegative: true,
    });
    if (values.help) {
        console.log(
            `${description}\n\nOptions:\n${Object.entries(options)
                .map(
                    ([k, v]) =>
                        `  --${k}${v.type === "string" ? " <value>" : ""}${v.default !== undefined ? ` (default: ${v.default})` : ""}`,
                )
                .join("\n")}`,
        );
        process.exit(0);
    }
    for (const [key, value] of Object.entries(spec))
        if (typeof value === "number") {
            values[key] = Number(values[key]);
            requireValue(Number.isFinite(values[key]), `--${key} must be a number`);
        }
    return Object.assign(values, { _: positionals });
}
export function required(options, ...keys) {
    for (const key of keys) requireValue(options[key] !== undefined && options[key] !== "", `--${key} is required`);
    return options;
}
export function run(argv, options = {}) {
    const { check = true, timeout = 900, inherit = false, ...rest } = options;
    const result = spawnSync(argv[0], argv.slice(1).map(String), {
        encoding: "utf8",
        maxBuffer: 256 * 1024 * 1024,
        timeout: timeout * 1000,
        stdio: inherit ? "inherit" : "pipe",
        ...rest,
    });
    if (result.error) throw result.error;
    const code = result.status ?? 128 + (result.signal ? 1 : 0);
    if (check && code !== 0) throw new Error(`${shellJoin(argv)} exited ${code}\n${result.stderr ?? ""}`);
    return { ...result, code, stdout: result.stdout ?? "", stderr: result.stderr ?? "" };
}
export async function runAsync(argv, options = {}) {
    const { check = true, timeout = 900, inherit = false, input, onSpawn, ...rest } = options;
    const child = spawn(argv[0], argv.slice(1).map(String), {
        stdio: inherit ? "inherit" : "pipe",
        detached: process.platform !== "win32",
        ...rest,
    });
    onSpawn?.(child);
    let stdout = "",
        stderr = "";
    child.stdout?.on("data", (data) => {
        stdout += data;
    });
    child.stderr?.on("data", (data) => {
        stderr += data;
    });
    child.stdin?.on("error", (error) => {
        if (error.code !== "EPIPE") child.kill("SIGKILL");
    });
    child.stdin?.end(input);
    const timer = setTimeout(() => {
        try {
            if (process.platform === "win32" || rest.detached === false) child.kill("SIGKILL");
            else process.kill(-child.pid, "SIGKILL");
        } catch (error) {
            if (error.code !== "ESRCH") throw error;
        }
    }, timeout * 1000);
    try {
        const code = await new Promise((resolve, reject) => {
            child.once("error", reject);
            child.once("close", (code) => resolve(code ?? 137));
        });
        if (check && code !== 0) throw new Error(`${shellJoin(argv)} exited ${code}\n${stderr}`);
        return { code, stdout, stderr };
    } finally {
        clearTimeout(timer);
    }
}
export function* walk(dir, predicate = () => true, descend = () => true) {
    for (const entry of fs
        .readdirSync(dir, { withFileTypes: true })
        .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))) {
        const file = path.join(dir, entry.name);
        if (entry.isDirectory() && descend(file)) yield* walk(file, predicate, descend);
        else if (entry.isFile() && predicate(file)) yield file;
    }
}
export const isMain = (url) =>
    Boolean(
        process.argv[1] && fs.existsSync(process.argv[1]) && fs.realpathSync(process.argv[1]) === fileURLToPath(url),
    );
export function main(fn) {
    Promise.resolve()
        .then(fn)
        .catch((error) => {
            console.error(error.message);
            process.exitCode = 1;
        });
}
export const pinnedCommit = () =>
    read(path.join(ROOT, "Cargo.toml")).match(/\[workspace\.metadata\.typescript\][\s\S]*?\bcommit\s*=\s*"([^"]+)"/)[1];
export function random(seed = Date.now()) {
    let state = Number(seed) >>> 0;
    const next = () => {
        state = (state + 0x6d2b79f5) >>> 0;
        let t = Math.imul(state ^ (state >>> 15), 1 | state);
        t ^= t + Math.imul(t ^ (t >>> 7), 61 | t);
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
    next.int = (min, max) => min + Math.floor(next() * (max - min + 1));
    next.choice = (a) => a[next.int(0, a.length - 1)];
    next.shuffle = (a) => {
        for (let i = a.length - 1; i; i--) {
            const j = next.int(0, i);
            [a[i], a[j]] = [a[j], a[i]];
        }
        return a;
    };
    next.sample = (a, n) => next.shuffle([...a]).slice(0, n);
    return next;
}
export async function mapLimit(items, limit, fn) {
    requireValue(Number.isInteger(limit) && limit > 0, "concurrency must be a positive integer");
    const result = new Array(items.length);
    let next = 0;
    await Promise.all(
        Array.from({ length: Math.min(limit, items.length) }, async () => {
            while (next < items.length) {
                const i = next++;
                result[i] = await fn(items[i], i);
            }
        }),
    );
    return result;
}
