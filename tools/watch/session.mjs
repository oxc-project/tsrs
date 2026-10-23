// A streaming watch session with bounded waits and reliable file restoration.
import fs from "node:fs";
import path from "node:path";
import { spawn } from "node:child_process";
import { performance } from "node:perf_hooks";
import { read, write, runAsync, requireValue } from "../lib/cli.mjs";
export const STRINGS = "vs/base/common/strings.ts",
    LIFECYCLE = "vs/base/common/lifecycle.ts",
    CONSUMER = "vs/base/common/arrays.ts";
export const SIG_OLD = "export function isFalsyOrWhitespace(str: string | undefined): boolean {",
    SIG_NEW = "export function isFalsyOrWhitespace(str: string | undefined): number {";
export const normalized = (lines) => lines.filter((l) => l.trim());
export const diags = (lines) => lines.filter((l) => /: error TS\d+:/.test(l)).sort();
export class WatchSession {
    constructor(compiler, project, extra = [], exact = false) {
        this.compiler = path.resolve(compiler);
        this.project = path.resolve(project);
        this.originals = new Map();
        this.queue = [];
        this.waiter = null;
        this.build = process.env.BUILD;
        const mode = this.build ? ["-b", this.build, "--watch"] : ["--watch", "-p", "."];
        this.proc = spawn(this.compiler, exact ? extra : [...mode, "--pretty", "false", ...extra], {
            cwd: this.project,
            stdio: ["ignore", "pipe", "pipe"],
        });
        let buffer = "";
        const pump = (data) => {
            buffer += data;
            for (;;) {
                const i = buffer.indexOf("\n");
                if (i < 0) break;
                const text = buffer.slice(0, i).replace(/\r$|\x1b\[[\d;]*[A-Za-z]/g, "");
                buffer = buffer.slice(i + 1);
                if (process.env.LOG) fs.appendFileSync(process.env.LOG, text + "\n");
                this.queue.push({ t: performance.now(), text });
                this.wake();
            }
        };
        this.proc.stdout.on("data", pump);
        this.proc.stderr.on("data", pump);
        this.done = new Promise((resolve) => {
            this.proc.once("error", (error) => {
                this.failure = error;
                this.wake();
                resolve();
            });
            this.proc.once("close", (code) => {
                this.failure = new Error(`watch compiler exited ${code}`);
                this.wake();
                resolve();
            });
        });
        this.onSignal = () => {
            this.failure = new Error("watch interrupted");
            this.wake();
            this.restore();
            this.proc.kill("SIGTERM");
        };
        process.once("SIGINT", this.onSignal);
        process.once("SIGTERM", this.onSignal);
    }
    wake() {
        this.waiter?.();
        this.waiter = null;
    }
    file(rel) {
        const file = path.resolve(this.project, rel);
        requireValue(file.startsWith(this.project + path.sep), "watch target must be within the project");
        return file;
    }
    remember(rel) {
        const file = this.file(rel);
        if (!this.originals.has(file)) this.originals.set(file, fs.existsSync(file) ? fs.readFileSync(file) : null);
        return this.originals.get(file)?.toString() ?? "";
    }
    edit(rel, fn) {
        this.remember(rel);
        const file = this.file(rel);
        if (fn === null) fs.rmSync(file, { force: true });
        else write(file, fn(fs.existsSync(file) ? read(file) : ""));
    }
    restore() {
        for (const [file, content] of this.originals)
            if (content === null) fs.rmSync(file, { force: true });
            else fs.writeFileSync(file, content);
    }
    async next(timeout) {
        if (!this.queue.length) {
            if (this.failure) throw this.failure;
            let timer;
            try {
                await new Promise((resolve) => {
                    this.waiter = resolve;
                    timer = setTimeout(() => {
                        this.waiter = null;
                        resolve();
                    }, timeout);
                });
            } finally {
                clearTimeout(timer);
            }
        }
        if (!this.queue.length && this.failure) throw this.failure;
        return this.queue.shift();
    }
    async cycles({ quiet = 3000, timeout = 300000, first = false } = {}) {
        let lines = [],
            running = false,
            detected = null;
        const cycles = [];
        for (;;) {
            const item = await this.next(cycles.length && !running ? quiet : timeout);
            if (!item) {
                requireValue(cycles.length > 0, "watch timed out without a completed compilation");
                return cycles;
            }
            const { t, text } = item;
            if (/Starting compilation|File change detected|Projects in this build/.test(text)) {
                lines = [];
                running = true;
                detected ??= t;
                continue;
            }
            if (text.includes("Watching for file changes")) {
                cycles.push({ lines, detected, done: t, summary: text.split(" - ").at(-1) });
                lines = [];
                running = false;
                detected = null;
                if (first) return cycles;
                continue;
            }
            lines.push(text);
        }
    }
    async fresh(buildMode = false) {
        const mode =
            this.build && buildMode ? ["-b", this.build, "--verbose", "false"] : ["-p", this.build ?? ".", "--noEmit"];
        const r = await runAsync([this.compiler, ...mode, "--pretty", "false"], {
            cwd: this.project,
            check: false,
            timeout: 300,
        });
        requireValue([0, 1, 2].includes(r.code), "fresh compiler failed");
        return r.stdout.split(/\r?\n/);
    }
    async stop() {
        if (this.proc.exitCode === null && this.proc.signalCode === null && this.proc.pid) {
            this.proc.kill("SIGINT");
            let timer;
            try {
                await Promise.race([
                    this.done,
                    new Promise((resolve) => {
                        timer = setTimeout(() => {
                            this.proc.kill("SIGKILL");
                            resolve();
                        }, 10000);
                    }),
                ]);
            } finally {
                clearTimeout(timer);
            }
        }
        await this.done;
    }
    async close() {
        try {
            await this.stop();
        } finally {
            this.restore();
            process.removeListener("SIGINT", this.onSignal);
            process.removeListener("SIGTERM", this.onSignal);
        }
    }
}
export function positional(usage, min = 2) {
    const a = process.argv.slice(2);
    if (a.includes("--help") || a.includes("-h")) {
        console.log(usage + "\nEnvironment: BUILD=<config>, LOG=<transcript>.");
        return null;
    }
    requireValue(a.length >= min, usage);
    return a;
}
export const equal = (a, b) => JSON.stringify(a) === JSON.stringify(b);
export function differences(a, b) {
    for (const l of a.filter((l) => !b.includes(l)).slice(0, 5)) console.log("  watch only:", l.slice(0, 200));
    for (const l of b.filter((l) => !a.includes(l)).slice(0, 5)) console.log("  fresh only:", l.slice(0, 200));
}
