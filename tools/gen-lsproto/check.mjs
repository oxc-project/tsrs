#!/usr/bin/env node
import { ROOT, read, args, main } from "../lib/cli.mjs";
import path from "node:path";
import { screamingSnakeCase } from "../lib/rust-names.mjs";
const keywords = new Set(
    "type ref match loop mod in as fn impl move self static struct super trait true false use where while async await dyn box do final macro override priv typeof unsized virtual yield try gen const crate else enum extern for if let pub return unsafe abstract become break continue".split(
        " ",
    ),
);
const snake = (text) => {
    const s = text
        .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
        .replace(/([A-Z]+)([A-Z][a-z])/g, "$1_$2")
        .toLowerCase();
    return s + (keywords.has(s) ? "_" : "");
};
main(() => {
    args("Check that every generated Go LSP name has its Rust counterpart.");
    const go = read(path.join(ROOT, "ts-ref/tsc/internal/lsp/lsproto/lsp_generated.go")),
        rs = read(path.join(ROOT, "crates/tsrs_lsproto/src/lsp_generated.rs")),
        missing = [];
    let checked = 0;
    const need = (ok, what) => {
            checked++;
            if (!ok) missing.push(what);
        },
        matches = (text, pattern) => [...text.matchAll(pattern)].map((m) => m[1]);
    const structs = new Map([...rs.matchAll(/^pub struct (\w+)\s*\{\n(.*?)^\}/gms)].map((m) => [m[1], m[2]])),
        tuples = new Set(matches(rs, /^pub struct (\w+)\(/gm)),
        aliases = new Set(matches(rs, /^pub type (\w+) =/gm));
    const types = new Set([
            ...structs.keys(),
            ...tuples,
            ...aliases,
            ...matches(rs, /^pub struct (\w+);/gm),
            ...matches(rs, /^pub struct (\w+)\s*\{\s*\}/gm),
        ]),
        constants = new Map(),
        methods = new Map();
    for (const m of rs.matchAll(/^impl (\w+) \{\n(.*?)^\}/gms)) {
        for (const c of m[2].matchAll(/^    pub const (\w+): (\w+) =/gm)) {
            if (!constants.has(c[2])) constants.set(c[2], new Set());
            constants.get(c[2]).add(c[1]);
        }
        for (const f of matches(m[2], /^    (?:pub(?:\(crate\))? )?fn (\w+)/gm)) {
            if (!methods.has(m[1])) methods.set(m[1], new Set());
            methods.get(m[1]).add(f);
        }
    }
    const impls = new Set([...rs.matchAll(/^impl ([\w:]+) for (\w+) \{/gm)].map((m) => `${m[1]}:${m[2]}`)),
        top = new Set(matches(rs, /^pub const (\w+):/gm)),
        enums = new Set(matches(go, /^type (\w+) (?:int32|uint32|string)$/gm));
    for (const m of go.matchAll(/^type (\w+) struct \{\n(.*?)^\}/gms)) {
        need(types.has(m[1]), `type ${m[1]}`);
        for (const f of matches(m[2], /^\t(\w+) /gm))
            need(
                new RegExp(`^    pub ${snake(f)}:`, "m").test(structs.get(m[1]) ?? ""),
                `field ${m[1]}.${f} (${snake(f)})`,
            );
    }
    for (const name of matches(go, /^type (\w+) struct\{\}$/gm)) need(types.has(name), `type ${name}`);
    for (const name of enums) need(tuples.has(name), `enum ${name}`);
    for (const name of matches(go, /^type (\w+) = /gm)) need(aliases.has(name), `alias ${name}`);
    for (const block of matches(go, /^const \(\n(.*?)^\)/gms))
        for (const c of block.matchAll(/^\t(\w+)\s+(\w+) = /gm)) {
            const [, name, type] = c;
            if (type === "Method" || enums.has(type)) {
                const suffix = name.slice(type.length);
                const rust = screamingSnakeCase(suffix);
                need(constants.get(type)?.has(rust), `const ${name} -> ${type}::${rust}`);
            } else missing.push(`unexpected const ${name} ${type}`);
        }
    for (const name of matches(go, /^var (\w+Info) = (?:Request|Notification)Info/gm))
        need(top.has(screamingSnakeCase(name)), `var ${name} -> ${screamingSnakeCase(name)}`);
    for (const m of go.matchAll(/^func \((\w+) \*?(\w+)\) (\w+)\(/gm)) {
        const [, , type, method] = m,
            trait = {
                MarshalJSONTo: "Json",
                UnmarshalJSONFrom: "Json",
                TextDocumentURI: "HasTextDocumentURI",
                TextDocumentPosition: "HasTextDocumentPosition",
                GetLocation: "HasLocation",
                GetLocations: "HasLocations",
                Error: "std::error::Error",
            }[method];
        need(
            trait
                ? impls.has(`${trait}:${type}`)
                : method === "String"
                  ? methods.get(type)?.has("string") && impls.has(`fmt::Display:${type}`)
                  : methods.get(type)?.has(snake(method)),
            `${type}.${method}`,
        );
    }
    for (const line of missing) console.log("missing:", line);
    console.log(`checked ${checked} names, ${missing.length} missing`);
    process.exitCode = missing.length ? 1 : 0;
});
