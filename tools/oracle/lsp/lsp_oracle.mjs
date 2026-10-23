#!/usr/bin/env node
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { ROOT, args, read, write, writeJSON, walk, requireValue, main, isMain } from "../../lib/cli.mjs";
import { unified } from "../../lib/diff.mjs";
import { Server, TSRS_WORK, fileURI, languageID, identifierOffsets, offsetToPosition } from "../../lib/lsp.mjs";
export { Server, TSRS_WORK, fileURI, languageID, offsetToPosition };
export function materialize(names, out) {
    return names.map((name) => {
        const file = fs.existsSync(name)
            ? name
            : ["compiler", "conformance"]
                  .flatMap((sub) => [...walk(path.join(ROOT, "ts-ref/tsc/testdata/tests/cases", sub))])
                  .find(
                      (f) => path.basename(f) === (name.endsWith(".ts") || name.endsWith(".tsx") ? name : name + ".ts"),
                  );
        requireValue(file, `conformance case not found: ${name}`);
        const dir = path.join(out, path.basename(file, path.extname(file))),
            options = {},
            files = [];
        let current = path.basename(file),
            lines = [];
        for (const line of read(file).split("\n")) {
            const m = line.match(/^\s*\/\/\s*@(\w+)\s*:\s*(.*?)\s*$/);
            if (!m) {
                lines.push(line);
                continue;
            }
            const [, key, value] = m;
            if (key.toLowerCase() === "filename") {
                if (lines.some((l) => l.trim())) files.push([current, lines.join("\n")]);
                current = value;
                lines = [];
            } else
                options[key] = /^(true|false)$/i.test(value)
                    ? value.toLowerCase() === "true"
                    : /^\d+$/.test(value)
                      ? Number(value)
                      : ["lib", "types", "rootdirs", "typeroots"].includes(key.toLowerCase())
                        ? value
                              .split(",")
                              .map((v) => v.trim())
                              .filter(Boolean)
                        : value.split(",")[0].trim();
        }
        files.push([current, lines.join("\n")]);
        for (const [name, content] of files) {
            const target = path.resolve(dir, name.replace(/^\/+/, ""));
            requireValue(target.startsWith(path.resolve(dir) + path.sep), "fixture path escapes scratch directory");
            write(target, content);
        }
        if (!files.some(([f]) => path.basename(f) === "tsconfig.json"))
            writeJSON(path.join(dir, "tsconfig.json"), { compilerOptions: options });
        return dir;
    });
}
function collect(root, patterns, max) {
    return [
        ...walk(
            root,
            (f) => /\.(ts|tsx|mts|cts|js|jsx)$/.test(f),
            (d) => path.basename(d) !== "node_modules" && !path.basename(d).startsWith("."),
        ),
    ]
        .filter((f) => !patterns.length || patterns.some((p) => path.matchesGlob(path.relative(root, f), p)))
        .sort()
        .slice(0, max);
}
async function session(server, root, allFiles, a, results) {
    const caps = {
        textDocument: {
            synchronization: { didSave: true },
            diagnostic: { dynamicRegistration: false },
            hover: { contentFormat: ["markdown", "plaintext"] },
            definition: { linkSupport: true },
            documentSymbol: { hierarchicalDocumentSymbolSupport: true },
            publishDiagnostics: { relatedInformation: true, versionSupport: true },
        },
        workspace: { configuration: true, workspaceFolders: true },
        general: { positionEncodings: ["utf-16"] },
    };
    if (a["watch-client"]) caps.workspace.didChangeWatchedFiles = { dynamicRegistration: true };
    results.push([
        "initialize",
        await server.request(
            "initialize",
            {
                processId: null,
                rootUri: fileURI(root),
                workspaceFolders: [{ uri: fileURI(root), name: path.basename(root) }],
                capabilities: caps,
            },
            a.timeout,
        ),
    ]);
    server.notify("initialized", {});
    const closed = allFiles.filter((f) => a.closed.some((p) => path.matchesGlob(path.relative(root, f), p))),
        files = allFiles.filter((f) => !closed.includes(f)),
        texts = new Map(files.map((f) => [f, read(f)]));
    for (const f of files)
        server.notify("textDocument/didOpen", {
            textDocument: { uri: fileURI(f), languageId: languageID(f), version: 1, text: texts.get(f) },
        });
    async function requests(tag) {
        for (const f of files) {
            const rel = path.relative(root, f),
                uri = fileURI(f),
                text = texts.get(f);
            for (const method of ["diagnostic", "documentSymbol"])
                if (a.requests.includes(method))
                    results.push([
                        `${tag}${method} ${rel}`,
                        await server.request(`textDocument/${method}`, { textDocument: { uri } }, a.timeout),
                    ]);
            for (const method of ["hover", "definition", "typeDefinition", "references", "signatureHelp", "completion"])
                if (a.requests.includes(method))
                    for (const off of identifierOffsets(text, a.positions)) {
                        const position = offsetToPosition(text, off),
                            params = {
                                textDocument: { uri },
                                position,
                                ...(method === "references" ? { context: { includeDeclaration: true } } : {}),
                            };
                        results.push([
                            `${tag}${method} ${rel}:${position.line + 1}:${position.character + 1}`,
                            await server.request(`textDocument/${method}`, params, a.timeout),
                        ]);
                    }
        }
    }
    await requests("");
    if (a.edits) {
        for (const f of files.slice(0, a["edit-files"])) {
            const text = texts.get(f),
                offsets = identifierOffsets(text, 3);
            if (!offsets.length) continue;
            const off = offsets[Math.floor(offsets.length / 2)],
                position = offsetToPosition(text, off);
            server.notify("textDocument/didChange", {
                textDocument: { uri: fileURI(f), version: 2 },
                contentChanges: [{ range: { start: position, end: position }, text: "x" }],
            });
            texts.set(f, text.slice(0, off) + "x" + text.slice(off));
            await requests(`edit(${path.relative(root, f)}) `);
            server.notify("textDocument/didChange", {
                textDocument: { uri: fileURI(f), version: 3 },
                contentChanges: [
                    { range: { start: position, end: { ...position, character: position.character + 1 } }, text: "" },
                ],
            });
            texts.set(f, text);
        }
        await requests("reverted ");
    }
    if (a["disk-edits"]) {
        await server.drain(a["watch-wait"]);
        const changed = async (f) => {
            if (a["watch-client"])
                server.notify("workspace/didChangeWatchedFiles", { changes: [{ uri: fileURI(f), type: 2 }] });
            await server.drain(a["watch-wait"]);
        };
        for (const f of closed.slice(0, a["disk-edits"])) {
            const text = read(f),
                match = a["disk-edit-at"] ? new RegExp(a["disk-edit-at"]).exec(text) : null,
                offsets = identifierOffsets(text, 3);
            if ((a["disk-edit-at"] && !match) || (!a["disk-edit-at"] && !offsets.length)) continue;
            const off = match ? match.index + match[0].length : offsets[Math.floor(offsets.length / 2)];
            try {
                write(f, text.slice(0, off) + "x" + text.slice(off));
                await changed(f);
                await requests(`disk(${path.relative(root, f)}) `);
            } finally {
                write(f, text);
            }
            await changed(f);
        }
        await requests("disk-reverted ");
    }
    for (const f of files) server.notify("textDocument/didClose", { textDocument: { uri: fileURI(f) } });
    await server.drain(a.drain);
    for (const uri of Object.keys(server.pushed).sort())
        results.push([`publishDiagnostics ${uri}`, server.pushed[uri]]);
    return results;
}
const canonical = (v) =>
    v && typeof v === "object"
        ? Array.isArray(v)
            ? v.map(canonical)
            : Object.fromEntries(
                  Object.keys(v)
                      .sort()
                      .map((k) => [k, canonical(v[k])]),
              )
        : v;
export function normalize(key, message) {
    if (message === null) return null;
    const msg = structuredClone(message);
    delete msg.id;
    delete msg.jsonrpc;
    if (key === "initialize" && msg.result) delete msg.result.serverInfo;
    if (/(^| )completion /.test(key) && Array.isArray(msg.result?.items))
        msg.result.items.sort((a, b) => JSON.stringify(canonical(a)).localeCompare(JSON.stringify(canonical(b))));
    if (msg.error && typeof msg.error === "object") msg.error = { code: msg.error.code };
    return canonical(msg);
}
if (isMain(import.meta.url))
    main(async () => {
        const a = args("Drive two language servers with an identical sequential session.", {
            a: path.join(TSRS_WORK, "bin/tsgo-ref") + " --lsp -stdio",
            b: path.join(ROOT, "target/release/tsrs") + " --lsp -stdio",
            project: null,
            files: [],
            conformance: [],
            "max-files": 20,
            requests: "diagnostic,hover,definition",
            positions: 40,
            edits: false,
            "edit-files": 2,
            timeout: 120,
            drain: 1,
            show: 5,
            dump: null,
            closed: [],
            "disk-edits": 0,
            "disk-edit-at": null,
            "watch-client": false,
            "watch-wait": 2,
        });
        requireValue(a.project || a.conformance.length, "--project or --conformance is required");
        const temp = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-lsp-")),
            roots = a.project ? [path.resolve(a.project)] : materialize(a.conformance, temp);
        let bad = 0,
            shown = 0;
        a.requests = a.requests.split(",");
        try {
            for (const root of roots) {
                const files = collect(root, a.files, a["max-files"]),
                    sessions = [];
                for (const [name, cmd] of [
                    ["a", a.a],
                    ["b", a.b],
                ]) {
                    const server = new Server(
                        name,
                        cmd,
                        root,
                        path.join(a.dump ?? temp, `${path.basename(root)}.${name}.log`),
                    );
                    const results = [];
                    sessions.push(results);
                    try {
                        await session(server, root, files, a, results);
                    } catch (error) {
                        bad++;
                        console.error(`${root}: server ${name} failed: ${error.message}`);
                    } finally {
                        await server.close();
                    }
                }
                const left = new Map(sessions[0]),
                    right = new Map(sessions[1]),
                    counts = new Map();
                for (const key of new Set([...left.keys(), ...right.keys()])) {
                    const l = JSON.stringify(normalize(key, left.get(key) ?? null), null, 2),
                        r = JSON.stringify(normalize(key, right.get(key) ?? null), null, 2),
                        method = key.split(" ")[0],
                        c = counts.get(method) ?? { requests: 0, equal: 0, different: 0 };
                    c.requests++;
                    if (l === r) c.equal++;
                    else {
                        c.different++;
                        bad++;
                        if (shown++ < a.show) console.log(`=== ${key}\n${unified(l + "\n", r + "\n", "a", "b")}`);
                    }
                    counts.set(method, c);
                }
                for (const [method, c] of counts)
                    console.log(`${method}: ${c.requests} requests, ${c.equal} equal, ${c.different} different`);
            }
        } finally {
            fs.rmSync(temp, { recursive: true, force: true });
        }
        process.exitCode = bad ? 1 : 0;
    });
