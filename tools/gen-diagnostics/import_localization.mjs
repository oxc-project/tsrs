#!/usr/bin/env node
// Import completed OneLoc translations, retaining existing pending translations.
import fs from "node:fs";
import { args, json, read, write, required, requireValue, isMain, main } from "../lib/cli.mjs";
export function readMessages(file) {
    const value = json(file);
    requireValue(
        value &&
            typeof value === "object" &&
            !Array.isArray(value) &&
            Object.values(value).every((v) => typeof v === "string"),
        `${file}: expected a JSON object of strings`,
    );
    return value;
}
// OneLoc uses a small XML vocabulary; validate the full document before reading resources.
function parseDocument(source, file) {
    const fail = () => {
        throw new Error(`${file}: malformed XML`);
    };
    const decode = (text) =>
        text.replace(/&([^;]*);|&/g, (whole, entity) => {
            const named = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" };
            if (Object.hasOwn(named, entity)) return named[entity];
            if (!/^#(?:x[\da-fA-F]+|\d+)$/.test(entity ?? "")) fail();
            const code = entity[1] === "x" ? parseInt(entity.slice(2), 16) : Number(entity.slice(1));
            if (
                !(
                    code === 9 ||
                    code === 10 ||
                    code === 13 ||
                    (code >= 32 && code <= 0xd7ff) ||
                    (code >= 0xe000 && code <= 0xfffd) ||
                    (code >= 0x10000 && code <= 0x10ffff)
                )
            )
                fail();
            return String.fromCodePoint(code);
        });
    const root = { children: [] },
        stack = [root];
    const tokens =
        /<!--[\s\S]*?-->|<\?[\s\S]*?\?>|<!\[CDATA\[[\s\S]*?\]\]>|<\/?[\p{L}_:][\p{L}\p{N}_.:-]*(?:[^<>"']|"[^"<]*"|'[^'<]*')*>|[^<]+/guy;
    let cursor = 0;
    while (cursor < source.length) {
        tokens.lastIndex = cursor;
        const m = tokens.exec(source);
        if (!m) fail();
        cursor = tokens.lastIndex;
        const token = m[0];
        if (token.startsWith("<!--")) {
            if (token.slice(4, -3).includes("--")) fail();
            continue;
        }
        if (token.startsWith("<?")) continue;
        if (token.startsWith("<![CDATA[")) {
            if (stack.length === 1) fail();
            continue;
        }
        if (!token.startsWith("<")) {
            decode(token);
            if (stack.length === 1 && token.trim()) fail();
            continue;
        }
        if (token.startsWith("</")) {
            const close = /^<\/([\p{L}_:][\p{L}\p{N}_.:-]*)\s*>$/u.exec(token);
            if (!close || stack.length === 1 || stack.at(-1).qualified !== close[1]) fail();
            stack.pop();
            continue;
        }
        const start = /^<([\p{L}_:][\p{L}\p{N}_.:-]*)/u.exec(token),
            qualified = start[1];
        const node = { qualified, name: qualified.split(":").at(-1), attributes: Object.create(null), children: [] };
        let rest = token.slice(start[0].length);
        while (!/^\s*\/?>$/.test(rest)) {
            const attr = /^\s+([\p{L}_:][\p{L}\p{N}_.:-]*)\s*=\s*("([^"]*)"|'([^']*)')/u.exec(rest);
            if (!attr || Object.hasOwn(node.attributes, attr[1])) fail();
            node.attributes[attr[1]] = decode(attr[3] ?? attr[4]);
            rest = rest.slice(attr[0].length);
        }
        stack.at(-1).children.push(node);
        if (!/\/\s*>$/.test(rest)) stack.push(node);
    }
    if (stack.length !== 1 || root.children.length !== 1 || root.children[0].name !== "LCX") fail();
    return root.children[0];
}
export function readMetadata(file) {
    const document = parseDocument(read(file).replace(/^\uFEFF/, ""), file);
    const messages = new Map();
    function visit(parent) {
        for (const item of parent.children) {
            if (item.name !== "Item") continue;
            const message = item.children.find((n) => n.name === "Str");
            if (message) {
                const id = item.attributes.ItemId ?? "";
                requireValue(id.startsWith(";") && id.length > 1, `${file}: invalid resource ID ${id}`);
                requireValue(!messages.has(id.slice(1)), `${file}: duplicate resource ID ${id}`);
                const target = message.children.find((n) => n.name === "Tgt");
                messages.set(id.slice(1), target ? (target.attributes.Stat ?? "") : null);
            }
            visit(item);
        }
    }
    visit(document);
    return messages;
}
export function importLocalization(source, handback, output) {
    const english = readMessages(source),
        returned = readMessages(handback),
        metadata = readMetadata(handback + ".lct");
    const existing = Object.fromEntries(
        Object.entries(fs.existsSync(output) ? readMessages(output) : {}).filter(([k]) => Object.hasOwn(english, k)),
    );
    for (const [key, text] of Object.entries(returned)) {
        if (!Object.hasOwn(english, key)) continue;
        const status = metadata.get(key);
        requireValue(status !== null && status !== undefined, `${handback}: missing localization metadata for ${key}`);
        if (status === "Loc") existing[key] = text;
        else
            requireValue(
                ["", "Update"].includes(status),
                `${handback}: unknown localization status ${status} for ${key}`,
            );
    }
    const active = Object.fromEntries(
        Object.keys(existing)
            .sort()
            .filter((k) => existing[k] !== english[k])
            .map((k) => [k, existing[k]]),
    );
    write(output, JSON.stringify(active, null, 2).replaceAll("\n", "\r\n"));
}
if (isMain(import.meta.url))
    main(() => {
        const a = required(
            args("Import OneLoc handback translations.", { source: null, handback: null, out: null }),
            "source",
            "handback",
            "out",
        );
        importLocalization(a.source, a.handback, a.out);
    });
