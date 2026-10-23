import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { spawn } from "node:child_process";
import { ROOT, write, sleep, shellJoin } from "./cli.mjs";
export const TSRS_WORK = process.env.TSRS_WORK ?? path.resolve(ROOT, "../..");
export const fileURI = (file) => pathToFileURL(path.resolve(file)).href;
export const languageID = (file) =>
    file.endsWith(".tsx")
        ? "typescriptreact"
        : file.endsWith(".jsx")
          ? "javascriptreact"
          : /\.[mc]?js$/.test(file)
            ? "javascript"
            : "typescript";
export const offsetToPosition = (text, off) => ({
    line: text.slice(0, off).split("\n").length - 1,
    character: off - (text.lastIndexOf("\n", off - 1) + 1),
});
export const stripCode = (text) =>
    text.replace(/\/\/[^\n]*|\/\*[\s\S]*?\*\/|"(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'|`(?:[^`\\\n]|\\.)*`/g, (s) =>
        s.replace(/[^\n]/g, " "),
    );
export function identifierOffsets(text, limit) {
    const offsets = [...stripCode(text).matchAll(/[A-Za-z_$][A-Za-z0-9_$]*/g)].map((m) => m.index);
    return offsets.length <= limit
        ? offsets
        : Array.from({ length: limit }, (_, i) => offsets[Math.floor((i * offsets.length) / limit)]);
}
export class Server {
    constructor(name, command, cwd, log, env = process.env, answer = true) {
        this.name = name;
        this.log = log;
        this.nextID = 0;
        this.transcript = [];
        this.pushed = {};
        this.pending = new Map();
        this.buffer = Buffer.alloc(0);
        this.error = "";
        this.answer = answer;
        this.proc = spawn("/bin/sh", ["-c", `exec ${Array.isArray(command) ? shellJoin(command) : command}`], {
            cwd,
            env,
            stdio: ["pipe", "pipe", "pipe"],
        });
        this.done = new Promise((resolve) => {
            this.proc.once("error", (e) => {
                this.error += e.message;
                this.rejectPending(e);
                resolve(127);
            });
            this.proc.once("close", (code, signal) => {
                this.rejectPending(new Error(`${name}: server closed stdout (exit ${code ?? signal})`));
                resolve(code ?? 137);
            });
        });
        this.proc.stderr.on("data", (chunk) => {
            this.error += chunk;
        });
        this.proc.stdin.on("error", (e) => this.rejectPending(e));
        this.proc.stdout.on("data", (chunk) => {
            try {
                this.buffer = Buffer.concat([this.buffer, chunk]);
                this.consume();
            } catch (error) {
                this.rejectPending(error);
                this.proc.kill("SIGKILL");
            }
        });
    }
    rejectPending(error) {
        this.failure = error;
        for (const { reject } of this.pending.values()) reject(error);
        this.pending.clear();
    }
    consume() {
        for (;;) {
            const sep = this.buffer.indexOf("\r\n\r\n");
            if (sep < 0) return;
            const match = this.buffer
                .subarray(0, sep)
                .toString()
                .match(/^Content-Length:\s*(\d+)\s*$/im);
            if (!match) throw new Error("missing LSP Content-Length");
            const length = Number(match[1]);
            if (this.buffer.length < sep + 4 + length) return;
            const msg = JSON.parse(this.buffer.subarray(sep + 4, sep + 4 + length).toString());
            this.buffer = this.buffer.subarray(sep + 4 + length);
            this.handle(msg);
        }
    }
    handle(msg) {
        if (msg.method !== undefined) {
            if (msg.id !== undefined) {
                this.transcript.push(["<-req", msg]);
                if (this.answer)
                    this.send({
                        jsonrpc: "2.0",
                        id: msg.id,
                        result:
                            msg.method === "workspace/configuration" ? (msg.params?.items ?? []).map(() => null) : null,
                    });
            } else if (
                !["window/logMessage", "$/progress", "telemetry/event", "window/showMessage", "$/logTrace"].includes(
                    msg.method,
                )
            ) {
                this.transcript.push(["<-", msg]);
                if (msg.method === "textDocument/publishDiagnostics") this.pushed[msg.params.uri] = msg.params;
            }
            return;
        }
        this.transcript.push(["<-", msg]);
        const pending = this.pending.get(msg.id);
        if (pending) {
            this.pending.delete(msg.id);
            pending.resolve(msg);
        }
    }
    send(msg) {
        if (this.failure) throw this.failure;
        const body = Buffer.from(JSON.stringify(msg));
        this.proc.stdin.write(Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`), body]));
        this.transcript.push(["->", msg]);
    }
    notify(method, params) {
        this.send({ jsonrpc: "2.0", method, params });
    }
    async request(method, params, timeout = 120) {
        const id = ++this.nextID;
        let timer;
        try {
            return await new Promise((resolve, reject) => {
                timer = setTimeout(() => {
                    this.pending.delete(id);
                    resolve({ error: { code: "timeout", message: `no response to ${method} within ${timeout}s` } });
                }, timeout * 1000);
                this.pending.set(id, { resolve, reject });
                this.send({ jsonrpc: "2.0", id, method, ...(params === null ? {} : { params }) });
            });
        } finally {
            clearTimeout(timer);
        }
    }
    drain(seconds) {
        return sleep(seconds * 1000);
    }
    async close(exitTimeout = 10) {
        let timer;
        try {
            try {
                if (!this.failure) {
                    await this.request("shutdown", null, 30);
                    this.notify("exit", null);
                }
            } catch (error) {
                this.error += `\nShutdown: ${error.message}\n`;
            }
            await Promise.race([
                this.done,
                new Promise((resolve) => {
                    timer = setTimeout(() => {
                        this.proc.kill("SIGKILL");
                        resolve();
                    }, exitTimeout * 1000);
                }),
            ]);
        } finally {
            clearTimeout(timer);
            if (this.proc.exitCode === null) this.proc.kill("SIGKILL");
            await this.done;
            if (this.log) {
                write(this.log, this.transcript.map(([d, m]) => d + " " + JSON.stringify(m) + "\n").join(""));
                write(this.log + ".stderr", this.error);
            }
        }
    }
}
