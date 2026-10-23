#!/usr/bin/env node
import path from "node:path";
import { ROOT, args, read, run, writeJSON, required, requireValue, main } from "../lib/cli.mjs";
import { Server, TSRS_WORK, fileURI, languageID, offsetToPosition } from "../lib/lsp.mjs";
main(async () => {
    const a = required(
        args("Measure language-server RSS during incremental edits.", {
            server: "tsrs",
            cmd: null,
            project: null,
            file: null,
            edits: 200,
            every: 10,
            timeout: 900,
            settle: 0,
            log: path.join(ROOT, "target/scratch/mem/lsp-mem"),
            json: null,
            env: [],
            completion: false,
            "exit-timeout": 10,
        }),
        "project",
        "file",
    );
    requireValue(a.every > 0 && a.edits >= 0, "invalid edit/sample count");
    const cmd =
            a.cmd ??
            (a.server === "tsgo" ? path.join(TSRS_WORK, "bin/tsgo-ref") : path.join(ROOT, "target/release/tsrs")) +
                " --lsp -stdio",
        root = path.resolve(a.project),
        file = path.join(root, a.file),
        text = read(file),
        m = text.match(/^([ \t]+)return\b/m);
    requireValue(m, "no return statement inside a function body found");
    const at = m.index + m[1].length,
        identifiers = [...text.slice(0, at).matchAll(/\b[A-Za-z_]\w{3,}\b/g)].map((m) => m.index),
        hov = identifiers[Math.floor(identifiers.length / 2)] ?? 0,
        env = { ...process.env };
    for (const kv of a.env) {
        const i = kv.indexOf("=");
        requireValue(i > 0, "--env requires KEY=VALUE");
        env[kv.slice(0, i)] = kv.slice(i + 1);
    }
    const server = new Server(a.server, cmd, root, a.log, env),
        uri = fileURI(file),
        samples = [],
        started = performance.now(),
        elapsed = () => (performance.now() - started) / 1000;
    let errors = 0;
    const sample = (n) => {
        const rss = Number(run(["ps", "-o", "rss=", "-p", server.proc.pid], { check: false }).stdout.trim() || 0);
        samples.push({ edits: n, rss_kib: rss, t: Number(elapsed().toFixed(2)) });
        console.log(`edits ${n} rss ${(rss / 1024).toFixed(1)} MiB t ${elapsed().toFixed(1)} s`);
    };
    try {
        await server.request(
            "initialize",
            {
                processId: null,
                rootUri: fileURI(root),
                workspaceFolders: [{ uri: fileURI(root), name: path.basename(root) }],
                capabilities: {
                    textDocument: {
                        diagnostic: { dynamicRegistration: false },
                        hover: { contentFormat: ["markdown", "plaintext"] },
                        completion: {
                            completionItem: {
                                snippetSupport: true,
                                resolveSupport: { properties: ["documentation", "detail", "additionalTextEdits"] },
                            },
                        },
                    },
                    workspace: { configuration: true, workspaceFolders: true },
                    general: { positionEncodings: ["utf-16"] },
                },
            },
            a.timeout,
        );
        server.notify("initialized", {});
        server.notify("textDocument/didOpen", {
            textDocument: { uri, languageId: languageID(file), version: 1, text },
        });
        const first = await server.request("textDocument/diagnostic", { textDocument: { uri } }, a.timeout);
        requireValue(!first.error, `first diagnostic failed: ${JSON.stringify(first.error)}`);
        sample(0);
        const typed = "let zz = 1; ";
        let cur = text,
            n = 0,
            direction = 1;
        for (let i = 1; i <= a.edits; i++) {
            const start = at + n - (direction < 0 ? 1 : 0),
                end = start + (direction < 0 ? 1 : 0),
                inserted = direction > 0 ? typed[n] : "",
                change = {
                    range: { start: offsetToPosition(cur, start), end: offsetToPosition(cur, end) },
                    text: inserted,
                };
            cur = cur.slice(0, start) + inserted + cur.slice(end);
            n += direction;
            if (n === typed.length || n === 0) direction *= -1;
            server.notify("textDocument/didChange", {
                textDocument: { uri, version: i + 1 },
                contentChanges: [change],
            });
            for (const method of ["diagnostic", "hover", ...(a.completion ? ["completion"] : [])]) {
                const r = await server.request(
                    `textDocument/${method}`,
                    {
                        textDocument: { uri },
                        ...(method === "diagnostic"
                            ? {}
                            : { position: offsetToPosition(cur, method === "completion" ? at + n : hov) }),
                    },
                    a.timeout,
                );
                if (r.error) errors++;
            }
            if (i % a.every === 0) sample(i);
        }
        if (a.settle) {
            await server.drain(a.settle);
            sample(a.edits);
        }
    } finally {
        await server.close(a["exit-timeout"]);
    }
    console.log(
        `summary ${a.server}: start ${(samples[0].rss_kib / 1024).toFixed(1)} MiB, end ${(samples.at(-1).rss_kib / 1024).toFixed(1)} MiB, peak ${(Math.max(...samples.map((s) => s.rss_kib)) / 1024).toFixed(1)} MiB, ${errors} errors`,
    );
    if (a.json) writeJSON(a.json, { server: a.server, cmd, file: a.file, samples, errors });
});
