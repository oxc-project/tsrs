#!/usr/bin/env node
import path from "node:path";
import { args, read, json, requireValue, main } from "../../lib/cli.mjs";
import { Server, fileURI, languageID, offsetToPosition } from "../../lib/lsp.mjs";
import { referenceBinary } from "../../lib/compiler.mjs";
main(async () => {
    const a = args("Record completion responses. <project> <cases.json>");
    requireValue(a._.length === 2, "project and cases file are required");
    const root = path.resolve(a._[0]),
        spec = json(a._[1]),
        server = new Server("reference", [referenceBinary(), "--lsp", "-stdio"], root),
        texts = {},
        results = {};
    try {
        await server.request("initialize", {
            processId: null,
            rootUri: fileURI(root),
            capabilities: spec.capabilities,
            initializationOptions: { userPreferences: spec.preferences },
        });
        server.notify("initialized", {});
        for (const f of spec.files) {
            texts[f] = read(path.join(root, f));
            server.notify("textDocument/didOpen", {
                textDocument: {
                    uri: fileURI(path.join(root, f)),
                    languageId: languageID(f),
                    version: 1,
                    text: texts[f],
                },
            });
        }
        for (const c of spec.cases) {
            let params;
            if (c.method === "completionItem/resolve") {
                params = results[c.from].items.find((i) => i.label === c.label);
                requireValue(params, `completion not found: ${c.label}`);
            } else {
                const text = texts[c.file],
                    needle = c.caret.replace("|", ""),
                    off = text.indexOf(needle);
                requireValue(off >= 0 && text.indexOf(needle, off + 1) < 0, `ambiguous caret ${c.caret}`);
                params = {
                    textDocument: { uri: fileURI(path.join(root, c.file)) },
                    position: offsetToPosition(text, off + c.caret.indexOf("|")),
                };
                if (c.method === "textDocument/completion") params.context = c.context ?? { triggerKind: 1 };
                if (c.method === "textDocument/_vs_onAutoInsert")
                    params = { _vs_textDocument: params.textDocument, _vs_position: params.position, _vs_ch: c.ch };
            }
            const r = await server.request(c.method, params);
            results[c.id] = r.result;
            console.log(
                JSON.stringify({ id: c.id, result: r.result ?? null, error: r.error ?? null }).replaceAll(root, ""),
            );
        }
    } finally {
        server.proc.kill("SIGKILL");
        await server.done;
    }
});
