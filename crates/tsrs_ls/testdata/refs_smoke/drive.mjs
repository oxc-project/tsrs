#!/usr/bin/env node
import path from "node:path";
import { args, read, json, requireValue, main } from "../../../../tools/lib/cli.mjs";
import { Server, fileURI, offsetToPosition } from "../../../../tools/lib/lsp.mjs";
import { referenceBinary } from "../../../../tools/lib/compiler.mjs";
main(async () => {
    const a = args("Record reference LSP responses. <project> <requests.json>");
    requireValue(a._.length === 2, "project and request file are required");
    const root = path.resolve(a._[0]),
        requests = json(a._[1]),
        base = fileURI(root) + "/",
        texts = {},
        server = new Server("reference", [referenceBinary(), "--lsp", "-stdio"], root);
    try {
        await server.request("initialize", {
            processId: null,
            rootUri: fileURI(root),
            capabilities: JSON.parse(process.env.CAPS ?? "{}"),
        });
        server.notify("initialized", {});
        for (const f of ["a.ts", "b.ts"]) {
            texts[f] = read(path.join(root, f));
            server.notify("textDocument/didOpen", {
                textDocument: { uri: base + f, languageId: "typescript", version: 1, text: texts[f] },
            });
        }
        for (const r of requests) {
            const off = texts[r.f].indexOf(r.at);
            requireValue(off >= 0, `marker not found: ${r.at}`);
            let params = {
                textDocument: { uri: base + r.f },
                position: offsetToPosition(texts[r.f], off + (r.off ?? 0)),
                ...JSON.parse(JSON.stringify(r.params ?? {}).replaceAll("file:///", base)),
            };
            if (r.m.startsWith("callHierarchy/")) {
                const prepared = await server.request("textDocument/prepareCallHierarchy", params);
                params = { item: prepared.result[0] };
            }
            const res = await server.request(r.m, params),
                value = res.error ? { error: res.error } : res.result;
            console.log(
                JSON.stringify(value)
                    .replaceAll(base, "file:///")
                    .replaceAll(root + "/", "/")
                    .replaceAll(root.toLowerCase() + "/", "/"),
            );
        }
    } finally {
        server.proc.kill("SIGKILL");
        await server.done;
    }
});
