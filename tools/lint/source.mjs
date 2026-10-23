#!/usr/bin/env node
// Source checks which Clippy cannot express. See docs/RUST.md, Source checks.
import fs from "node:fs";
import path from "node:path";
import assert from "node:assert/strict";
import { ROOT, args, read, write, walk, main } from "../lib/cli.mjs";
const dir = path.join(ROOT, "tools/lint");
const compact = (s) => s.trim().replace(/\s+/g, " ");
const codePart = (s) => s.split("//")[0];
function* sources() {
    for (const base of ["crates", "tasks"]) {
        const folder = path.join(ROOT, base);
        if (!fs.existsSync(folder)) continue;
        for (const entry of fs.readdirSync(folder).sort()) {
            const src = path.join(folder, entry, "src");
            if (!fs.existsSync(src)) continue;
            for (const file of walk(
                src,
                (f) => f.endsWith(".rs") && !/(?:_tests?\.rs|\/tests\.rs|\/tests[^/]*\/)/.test(f),
            )) {
                const text = read(file);
                if (/Code generated|@generated/.test(text.split("\n")[0])) continue;
                yield [path.relative(ROOT, file), text];
            }
        }
    }
}
function sendSync(rel, text) {
    return [
        ...text.matchAll(
            /\bunsafe\s+impl\b(?:\s*<(?<generics>[^{]*?)>)?\s+(?<trait>Send|Sync)\s+for\s+(?<type>[^{]+?)\s*\{/g,
        ),
    ]
        .filter(
            (m) =>
                !text
                    .slice(text.lastIndexOf("\n", m.index) + 1, m.index)
                    .trimStart()
                    .startsWith("//"),
        )
        .map((m) => [
            rel,
            m.groups.trait,
            (m.groups.generics ? `<${m.groups.generics.trim()}> ` : "") + compact(m.groups.type),
        ]);
}
function orderings(text) {
    const weak = ["Relaxed", "Acquire", "Release", "AcqRel"],
        imported = new Set();
    for (const m of text.matchAll(
        /\buse\s+std::sync::atomic::(?:\{[^}]*\bOrdering::(?:\{[^}]*\}|\w+)[^}]*\}|Ordering::(?:\{[^}]*\}|\w+))\s*;/g,
    ))
        for (const w of weak) if (new RegExp(`\\b${w}\\b`).test(m[0])) imported.add(w);
    const bare = imported.size ? new RegExp(`(?<![\\w:])(${[...imported].sort().join("|")})\\b`, "g") : null,
        lines = text.split("\n"),
        found = [];
    lines.forEach((line, i) => {
        const code = codePart(line);
        if (code.trimStart().startsWith("use ")) return;
        const hits =
            [...code.matchAll(/\bOrdering::(Relaxed|Acquire|Release|AcqRel)\b/g)].length +
            (bare ? [...code.matchAll(bare)].length : 0);
        if (hits && !lines.slice(Math.max(0, i - 3), i + 1).some((l) => l.includes("//")))
            for (let k = 0; k < hits; k++) found.push([i + 1, line.trim()]);
    });
    return found;
}
function matching(text, start, open, close, literals = false) {
    let depth = 0;
    for (let i = start; i >= 0 && i < text.length; i++) {
        const ch = text[i];
        if (literals && ch === '"') {
            for (i++; i < text.length && text[i] !== '"'; i++) if (text[i] === "\\") i++;
        } else if (literals && ch === "'" && (text[i + 2] === "'" || (text[i + 1] === "\\" && text[i + 3] === "'")))
            i = text.indexOf("'", i + 2);
        else if (ch === open) depth++;
        else if (ch === close && --depth === 0) return i + 1;
    }
    return text.length;
}
function refs(params) {
    const names = new Set();
    let depth = 0,
        cur = "";
    for (const ch of params + ",") {
        if ("<([".includes(ch)) depth++;
        else if (">)]".includes(ch)) depth--;
        if (ch === "," && depth === 0) {
            const p = cur.trim();
            cur = "";
            if (/^&\s*(?:'\w+\s+)?(?:mut\s+)?self$/.test(p)) names.add("self");
            const m = p.match(/^(?:mut\s+)?(\w+)\s*:\s*&/);
            if (m) names.add(m[1]);
        } else cur += ch;
    }
    return names;
}
function referenceFrees(text) {
    const code = text.split("\n").map(codePart).join("\n"),
        found = [];
    for (const m of code.matchAll(/\bfn\s+\w+\s*(?:<[^(]*?>)?\s*\(/g)) {
        const open = m.index + m[0].length - 1,
            close = matching(code, open, "(", ")"),
            brace = code.indexOf("{", close),
            semi = code.indexOf(";", close);
        if (brace < 0 || (semi >= 0 && semi < brace)) continue;
        const names = refs(code.slice(open + 1, close - 1)),
            end = matching(code, brace, "{", "}");
        for (const f of code
            .slice(brace, end)
            .matchAll(/\b(free!|free_slice!|free_raw|free_slice_ptr)\s*\(\s*&?\s*(\w+)/g))
            if (names.has(f[2])) {
                const n = code.slice(0, brace + f.index).split("\n").length,
                    line = text.split("\n")[n - 1];
                if (!/source\.(?:py|mjs): deliberate/.test(line)) found.push([n, line.trim()]);
            }
    }
    return found;
}
const literal = String.raw`\br(?<rh>#*)"[\s\S]*?"\k<rh>|b?"(?:[^"\\]|\\[\s\S])*"|b?'(?:[^'\\]|\\.)'`;
const itemHead = String.raw`(?:^|[\s;}])(?:pub(?:\([^)]*\))?\s+)?(?:(?:const|async|unsafe|extern\s+"[^"]*")\s+)*fn\s+(?<fn>\w+)|^[ \t]*(?:unsafe\s+)?impl\s*(?:<[^{=]*?>\s*)?(?<impl>[^{;]*?)\s*\{|(?:^|[\s;}])(?:pub(?:\([^)]*\))?\s+)?static\s+(?:mut\s+)?(?<static>\w+)\s*:|(?:^|[\s;}])macro_rules!\s+(?<macro>\w+)|#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(?<testmod>\w+)\s*\{`;
const comments = new RegExp(`(?<literal>${literal})|//[^\\n]*|/\\*[\\s\\S]*?\\*/`, "g");
function stripComments(text) {
    return text.replace(comments, (...a) => {
        const groups = a.at(-1),
            match = a[0];
        return groups.literal !== undefined ? match : "\n".repeat(match.split("\n").length - 1);
    });
}
function items(code) {
    const out = [],
        stack = [];
    let depth = 0,
        pending = null,
        nest = 0;
    for (const m of code.matchAll(new RegExp(`${literal}|[{}()\\[\\];]|${itemHead}`, "gm"))) {
        const t = m[0],
            g = m.groups,
            group = ["fn", "static", "macro", "impl", "testmod"].find((k) => g[k] !== undefined);
        if (["fn", "static", "macro"].includes(group)) {
            if (pending) out.push([pending[1], m.index, pending[0]]);
            pending = [`${group} ${g[group]}`, m.index];
            nest = 0;
        } else if (["impl", "testmod"].includes(group)) {
            if (pending) {
                out.push([pending[1], m.index, pending[0]]);
                pending = null;
            }
            depth++;
            stack.push([
                group === "impl" ? `impl ${compact(g.impl).replace(/\s*\bwhere\b.*$/, "")}` : `testmod ${g.testmod}`,
                depth,
                m.index,
            ]);
        } else if (t === "{") {
            depth++;
            if (pending && nest === 0) {
                stack.push([pending[0], depth, pending[1]]);
                pending = null;
            }
        } else if (t === "}") {
            if (stack.at(-1)?.[1] === depth) {
                const [label, , start] = stack.pop();
                out.push([start, m.index + t.length, label]);
            }
            depth--;
        } else if (t === "(" || t === "[") nest++;
        else if (t === ")" || t === "]") nest--;
        else if (t === ";" && pending && nest === 0) {
            out.push([pending[1], m.index, pending[0]]);
            pending = null;
        }
    }
    for (const [label, , start] of stack) out.push([start, code.length, label]);
    return out;
}
function enclosing(all, pos) {
    const inner = all
        .filter(([s, e, label]) => s <= pos && pos < e && !label.startsWith("testmod "))
        .sort((a, b) => a[1] - a[0] - (b[1] - b[0]));
    if (!inner.length) return "(file)";
    const label = inner[0][2];
    if (label.startsWith("fn ")) {
        const impl = inner.slice(1).find((it) => it[2].startsWith("impl "));
        return impl
            ? `${impl[2].slice(5).split(" for ").at(-1).replace(/<.*/, "").trim()}::${label.slice(3)}`
            : label.slice(3);
    }
    return label;
}
function* above(lines, i, limit) {
    yield i;
    for (let j = i - 1; j > Math.max(-1, i - limit); j--) {
        const code = codePart(lines[j]).trimEnd(),
            text = lines[j].trim();
        if (!text || (/[;{}]$/.test(code) && !/^(\/\/|#\[|#!\[)/.test(text))) return;
        yield j;
    }
}
function safety(lines, i, fn) {
    if (fn) {
        for (const j of above(lines, i, 24))
            if (j < i && lines[j].includes("# Safety"))
                for (let k = j + 1; k < i; k++)
                    if (lines[k].trim().startsWith("///") && lines[k].replace(/^\s*\/+[ \t]*/, "").trim())
                        return compact(lines[k].trim().replace(/^\/+/, "")).slice(0, 110);
        return "";
    }
    for (const j of above(lines, i, 10)) {
        const m = lines[j].match(/SAFETY:?\s*(.*)/);
        if (m) {
            let text = m[1].trim();
            if (!text && lines[j + 1]?.trim().startsWith("//")) text = lines[j + 1].trim().replace(/^\/+/, "").trim();
            return compact(text).slice(0, 110);
        }
        const r = lines[j].match(/reason\s*=\s*"([^"]*)"/);
        if (r) return compact(r[1]).slice(0, 110);
    }
    return "";
}
function classify(rel, item, kind, body) {
    const crate = rel.split("/")[1],
        name = path.basename(rel);
    if (
        crate === "tsrs_core" &&
        ["ptr.rs", "arena.rs", "reserve.rs", "memsplit.rs", "sharedgraph.rs", "frozen.rs"].includes(name)
    )
        return "core-allocator";
    if (kind === "extern") return "boundary";
    if (
        ["leak", "from_raw", "forget", "lifetime"].includes(kind) ||
        /free_program|free_unshared_program|SharedProgramData|\.free\(\)|free_checkers|free_project_reference_file_mapper|free_api_orchestrator|drop_replaced|free_replaced|Box::from_raw|Box::leak/.test(
            body,
        ) ||
        /^(assume_static$|free_|SharedProgramData::|Program::(drop|free)_replaced$)/.test(item)
    )
        return "ownership";
    return ["tsrs_fswatch", "tsrs_wasm", "tsrs_cli"].includes(crate) || name === "allocator.rs" ? "boundary" : "logic";
}
function unsafeSites(rel, text) {
    const code = stripComments(text),
        all = items(code),
        lines = text.split("\n"),
        rows = [];
    for (const m of code.matchAll(
        /\bunsafe\s*\{|\bunsafe\s+fn\s+(?<fn>\w+)|\bunsafe\s+impl\b|\bunsafe\s+extern\b|Box::leak\b|Box::from_raw\b|mem::forget\b|\btransmute\b|\bassume_static\b/g,
    )) {
        const kind = m.groups.fn
            ? "fn"
            : m[0].startsWith("unsafe")
              ? { "{": "block", i: "impl", e: "extern" }[m[0].slice(6).trimStart()[0]]
              : {
                    "Box::leak": "leak",
                    "Box::from_raw": "from_raw",
                    "mem::forget": "forget",
                    transmute: "transmute",
                    assume_static: "lifetime",
                }[m[0]];
        const n = code.slice(0, m.index).split("\n").length - 1,
            line = lines[n];
        if (line.trimStart().startsWith("use ") || (kind === "transmute" && line.includes("fn transmute"))) continue;
        const item =
                kind === "lifetime" && line.includes("fn assume_static") ? "assume_static" : enclosing(all, m.index),
            body =
                kind === "block" ? code.slice(m.index, matching(code, code.indexOf("{", m.index), "{", "}", true)) : "";
        rows.push([
            rel,
            item,
            kind,
            safety(lines, n, kind === "fn"),
            all.some(([s, e, label]) => label.startsWith("testmod ") && s <= m.index && m.index < e)
                ? "test"
                : classify(rel, item, kind, body),
        ]);
    }
    return rows;
}
const tsv = (file) =>
    fs.existsSync(file)
        ? read(file)
              .split("\n")
              .filter((l) => l.trim() && !l.startsWith("#"))
              .map((l) => l.split("\t"))
        : [];
const key = (row) => JSON.stringify(row);
const sortRows = (rows) =>
    rows.sort((a, b) => {
        for (let i = 0; i < Math.min(a.length, b.length); i++) {
            if (a[i] < b[i]) return -1;
            if (a[i] > b[i]) return 1;
        }
        return a.length - b.length;
    });
main(() => {
    const a = args("Check Send/Sync, atomic comments, reference frees and unsafe site inventories.", {
        update: false,
        "allow-increase": false,
        "self-test": false,
    });
    const inventory = [],
        atomics = new Map(),
        sites = new Map(),
        frees = [],
        seen = new Map();
    let files = 0;
    for (const [rel, text] of sources()) {
        files++;
        inventory.push(...sendSync(rel, text));
        frees.push(...referenceFrees(text).map((r) => [rel, ...r]));
        const found = orderings(text);
        if (found.length) atomics.set(rel, found);
        for (const [file, item, kind, safety, cls] of unsafeSites(rel, text)) {
            const id = key([file, item, kind, safety]),
                n = (seen.get(id) ?? 0) + 1;
            seen.set(id, n);
            sites.set(key([file, item, kind, n > 1 ? `${safety} #${n}`.trim() : safety]), cls);
        }
    }
    sortRows(inventory);
    const baseline = new Map(tsv(path.join(dir, "atomics.tsv")).map(([n, f]) => [f, Number(n)])),
        recorded = new Map(
            tsv(path.join(dir, "unsafe_sites.tsv")).map((r) => [
                key([r[0], r[1], r[2], r[4] ?? ""]),
                [r[3], r[5] ?? "", r[6] ?? ""],
            ]),
        );
    if (a["self-test"]) {
        assert.deepEqual(
            sendSync(
                "x.rs",
                "unsafe impl Send for A {}\nunsafe impl<T: ?Sized> Sync for P<T> {}\n// unsafe impl Send for C {}",
            ),
            [
                ["x.rs", "Send", "A"],
                ["x.rs", "Sync", "<T: ?Sized> P<T>"],
            ],
        );
        assert.deepEqual(
            referenceFrees("fn f(x: &[u8]) { free_slice!(x); }").map((r) => r[0]),
            [1],
        );
        assert.deepEqual(sendSync("x.rs", "impl Send for Safe {}"), []);
        assert.deepEqual(
            orderings(
                [
                    "use std::sync::atomic::{AtomicU32, Ordering::Relaxed};",
                    "fn f() {",
                    "    a.load(Ordering::Relaxed);",
                    "    b.load(Relaxed); // counter only",
                    "    // ids only need to be unique",
                    "    c.fetch_add(1, Relaxed);",
                    "    d.store(1, Ordering::SeqCst);",
                    "    let x = 1;",
                    "    e.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed);",
                    "}",
                ].join("\n"),
            ).map(([n]) => n),
            [3, 9, 9],
        );
        assert.deepEqual(orderings("enum Kind { Release }\nlet k = Kind::Release;"), []);
        assert.deepEqual(
            referenceFrees(
                [
                    "unsafe fn recycle(m: P<TypeMapper>, targets: &'static [P<Type>]) {",
                    "    tsrs_core::free!(m);",
                    "    tsrs_core::free_slice!(targets);",
                    "}",
                    "fn ok(targets: *const [P<Type>], n: usize) { unsafe { tsrs_core::free_slice_ptr(targets) } }",
                    "impl X { fn drop_me(&self) { unsafe { free_raw(self as *const X as usize, 8, 8, false) } } }",
                    "fn local() { let s: &[u8] = alloc_slice_recycled(&[1]); unsafe { free_slice!(s) } }",
                    "fn decl(s: &[u8]);",
                    "fn after(x: u32) { free!(p) }",
                ].join("\n"),
            ).map(([n]) => n),
            [3, 6],
        );
        assert.deepEqual(
            unsafeSites(
                "crates/x/src/x.rs",
                [
                    "impl Store {",
                    "    /// # Safety",
                    "    /// `i` is below `len`.",
                    "    #[inline]",
                    "    pub unsafe fn at(&self, i: usize) -> u32 {",
                    "        // SAFETY: the caller's contract.",
                    "        unsafe { *self.p.add(i) }",
                    "    }",
                    "    fn out(&self) -> [u64; 2] {",
                    "        // SAFETY: a plain (the key keeps the first line)",
                    "        // byte copy.",
                    "        let w = unsafe { std::mem::transmute::<[u8; 16], [u64; 2]>(self.b) };",
                    "        w",
                    "    }",
                    "}",
                    "fn leak(h: Host) -> &'static Host {",
                    '    #[expect(clippy::disallowed_methods, reason = "lives for the process")]',
                    "    Box::leak(Box::new(h))",
                    "}",
                    'fn fmt(s: &str) -> String { format!("{{") }',
                    'unsafe extern "C" { fn malloc_trim(pad: i32) -> i32; }',
                    "#[cfg(test)]",
                    "mod tests {",
                    "    fn t() { unsafe { touch() } }",
                    "}",
                ].join("\n"),
            ),
            [
                ["crates/x/src/x.rs", "Store::at", "fn", "`i` is below `len`.", "logic"],
                ["crates/x/src/x.rs", "Store::at", "block", "the caller's contract.", "logic"],
                ["crates/x/src/x.rs", "Store::out", "block", "a plain (the key keeps the first line)", "logic"],
                ["crates/x/src/x.rs", "Store::out", "transmute", "a plain (the key keeps the first line)", "logic"],
                ["crates/x/src/x.rs", "leak", "leak", "lives for the process", "ownership"],
                ["crates/x/src/x.rs", "(file)", "extern", "", "boundary"],
                ["crates/x/src/x.rs", "t", "block", "", "test"],
            ],
        );
        assert.equal(classify("crates/tsrs_core/src/arena.rs", "free_block", "fn", ""), "core-allocator");
        assert.equal(
            classify("crates/tsrs_fswatch/src/kqueue.rs", "close_fd", "block", "{ libc::close(fd) }"),
            "boundary",
        );
        assert.equal(
            classify(
                "crates/tsrs_execute/src/watcher.rs",
                "Watcher::install_program",
                "block",
                "{ free_program(old) }",
            ),
            "ownership",
        );
        assert(files > 100 && sites.size > 300);
        console.log("source checks self-test: ok");
        return;
    }
    if (a.update) {
        const save = (name, header, rows) =>
            write(path.join(dir, name), header + rows.map((r) => r.join("\t")).join("\n") + "\n");
        save(
            "send_sync.tsv",
            "# Every unsafe impl Send / Sync (tools/lint/source.mjs --update). Review each change.\n",
            inventory,
        );
        save(
            "unsafe_sites.tsv",
            "# Every unsafe site and lifetime escape (tools/lint/source.mjs --update; notes/unsafe-audit.md).\n# file\titem\tkind\tclass\tsafety\tverdict\tnote. --update keeps class, verdict and note of a row whose key\n# (file, item, kind, safety) still exists; a new site gets a class from its crate and verdict `unknown`.\n",
            sortRows(
                [...sites].map(([id, cls]) => {
                    const [f, item, kind, safety] = JSON.parse(id),
                        [c, verdict, note] = recorded.get(id) ?? [cls, "unknown", ""];
                    return [f, item, kind, c, safety, verdict, note];
                }),
            ),
        );
        const counts = [...atomics]
            .filter(([f]) => a["allow-increase"] || !fs.existsSync(path.join(dir, "atomics.tsv")) || baseline.has(f))
            .map(([f, rows]) => [
                a["allow-increase"] || !baseline.has(f) ? rows.length : Math.min(rows.length, baseline.get(f)),
                f,
            ])
            .sort((a, b) => a[0] - b[0] || (a[1] < b[1] ? -1 : 1));
        save(
            "atomics.tsv",
            "# Weakened atomic orderings without a comment, per file (tools/lint/source.mjs). Counts only go down.\n",
            counts,
        );
        console.log(`source checks: ${inventory.length} Send/Sync impls, ${sites.size} unsafe sites recorded`);
        return;
    }
    const problems = [];
    const old = new Set(tsv(path.join(dir, "send_sync.tsv")).map(key)),
        current = new Set(inventory.map(key));
    for (const [rel, n, line] of frees)
        problems.push(
            `${rel}:${n}: arena free of a reference parameter (take a P<T>, address or raw slice):\n  ${line}`,
        );
    for (const id of current)
        if (!old.has(id))
            problems.push(
                `new unsafe impl ${id}: justify it with a SAFETY comment, then run tools/lint/source.mjs --update`,
            );
    for (const id of old)
        if (!current.has(id)) problems.push(`unsafe impl gone: ${id}: run tools/lint/source.mjs --update`);
    for (const id of sites.keys())
        if (!recorded.has(id))
            problems.push(`new unsafe site: ${id}: state its class and verdict after tools/lint/source.mjs --update`);
    for (const [id, [cls, verdict]] of recorded) {
        if (!sites.has(id)) problems.push(`unsafe site gone: ${id}: run tools/lint/source.mjs --update`);
        if (
            !["boundary", "core-allocator", "ownership", "logic", "test"].includes(cls) ||
            !["keep", "safe-api-possible", "defect-risk", "unknown"].includes(verdict)
        )
            problems.push(`unsafe site ${id}: invalid class or verdict`);
    }
    for (const [file, rows] of atomics)
        if (rows.length > (baseline.get(file) ?? 0))
            problems.push(
                `${file}: ${rows.length} weakened atomic orderings without a comment (baseline ${baseline.get(file) ?? 0})\n${rows.map(([n, l]) => `  ${file}:${n}  ${l}`).join("\n")}`,
            );
    if (problems.length) {
        console.log(problems.join("\n"));
        process.exitCode = 1;
    } else
        console.log(
            `source checks: ok (${files} files, ${inventory.length} Send/Sync impls, ${[...atomics.values()].reduce((s, a) => s + a.length, 0)} uncommented orderings, ${sites.size} unsafe sites, no arena free of a reference parameter)`,
        );
});
