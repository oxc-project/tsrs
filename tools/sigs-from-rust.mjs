#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { ROOT, args, read, write, main } from "./lib/cli.mjs";
function stripCode(src) {
    const out = src.split("");
    let i = 0;
    const blank = (a, b) => {
        for (let k = a; k < Math.min(b, src.length); k++) if (out[k] !== "\n") out[k] = " ";
    };
    while (i < src.length) {
        let j;
        if (src.startsWith("//", i)) {
            j = src.indexOf("\n", i);
            if (j < 0) j = src.length;
        } else if (src.startsWith("/*", i)) {
            let depth = 1;
            j = i + 2;
            while (j < src.length && depth) {
                if (src.startsWith("/*", j)) {
                    depth++;
                    j += 2;
                } else if (src.startsWith("*/", j)) {
                    depth--;
                    j += 2;
                } else j++;
            }
        } else {
            const m = /^r(#*)"/.exec(src.slice(i));
            if (m && (i === 0 || !/[\w]/.test(src[i - 1]))) {
                const end = src.indexOf('"' + m[1], i + m[0].length);
                j = end < 0 ? src.length : end + m[1].length + 1;
            } else if (src[i] === '"') {
                j = i + 1;
                while (j < src.length && src[j] !== '"') {
                    if (src[j] === "\\") j++;
                    j++;
                }
                j++;
            } else if (src[i] === "'" && (src[i + 1] === "\\" || src[i + 2] === "'")) j = src.indexOf("'", i + 2) + 1;
        }
        if (j !== undefined && j > i) {
            blank(i, j);
            i = j;
        } else i++;
    }
    return out.join("");
}
function receiver(header) {
    let h = header.replace(/\s+/g, " ").trim().slice(4).trim();
    if (h.startsWith("<")) {
        let depth = 0;
        for (let i = 0; i < h.length; i++) {
            if (h[i] === "<") depth++;
            else if (h[i] === ">" && --depth === 0) {
                h = h.slice(i + 1).trim();
                break;
            }
        }
    }
    return /\bfor\b/.test(h)
        ? null
        : (h
              .match(/^[A-Za-z_][\w:]*/)?.[0]
              .split("::")
              .at(-1) ?? null);
}
function scan(file, rel) {
    const src = read(file),
        code = stripCode(src),
        lines = src.split("\n"),
        stack = [],
        rows = [],
        item =
            /(?:#\[cfg\(test\)\]\s*)?(?:pub(?:\([a-z]+\))?\s+)?(?:const\s+|unsafe\s+|extern\s+"C"\s+)*(impl\b|fn\s+([A-Za-z_]\w*)|mod\s+([A-Za-z_]\w*)|macro_rules!)/y;
    for (let i = 0; i < code.length; ) {
        if (code[i] === "{") {
            stack.push(["other"]);
            i++;
            continue;
        }
        if (code[i] === "}") {
            stack.pop();
            i++;
            continue;
        }
        item.lastIndex = i;
        const m = /[A-Za-z#_]/.test(code[i]) && (i === 0 || !/\w/.test(code[i - 1])) ? item.exec(code) : null;
        if (!m) {
            i++;
            continue;
        }
        let j = item.lastIndex,
            depth = 0;
        while (j < code.length) {
            const ch = code[j];
            if ("([<".includes(ch) && !(ch === "<" && code[j - 1] === "-")) depth++;
            else if (")]>".includes(ch) && !(ch === ">" && "-=".includes(code[j - 1]))) depth--;
            else if (depth <= 0 && "{;".includes(ch)) break;
            j++;
        }
        const header = code.slice(i, j),
            kind = m[1],
            inTest = stack.some((s) => s[0] === "test"),
            parent = stack.at(-1);
        let scope = ["other"];
        if (kind.startsWith("fn")) {
            if (!inTest && (!parent || (parent[0] === "impl" && parent[1] !== null))) {
                const sig = src
                        .slice(i, j)
                        .replace(/\s+/g, " ")
                        .trim()
                        .replace(/^#\[cfg\(test\)\]\s*/, "")
                        .replaceAll("( ", "(")
                        .replaceAll(" )", ")")
                        .replaceAll(", )", ")")
                        .replaceAll(",)", ")")
                        .replace(/([(,] ?)mut ([A-Za-z_]\w*):/g, "$1$2:"),
                    line = code.slice(0, i).split("\n").length - 1;
                let origin;
                for (let k = line - 1; k >= 0; k--) {
                    const s = lines[k].trim();
                    if (!/^(\/\/|#\[|\*|\/\*)/.test(s)) break;
                    origin ??= s.match(/\/\/\s*([a-z_]+\.go:\d+)/)?.[1];
                }
                rows.push([m[2], sig, origin ?? `rust:${rel}:${line + 1}`, parent ? parent[1] : "-"]);
            }
            scope = ["fn"];
        } else if (kind === "impl") scope = ["impl", inTest ? null : receiver(header)];
        else if (kind.startsWith("mod")) scope = [header.includes("cfg(test)") || m[3] === "tests" ? "test" : "other"];
        else if (kind === "macro_rules!") scope = ["test"];
        if (code[j] === "{") stack.push(scope);
        i = j + 1;
    }
    return rows;
}
main(() => {
    const a = args("Regenerate Rust signature reference tables. [repo-root]"),
        root = a._[0] ?? ROOT;
    for (const [crate, name] of [
        ["tsrs_checker", "checker"],
        ["tsrs_pseudochecker", "pseudochecker"],
        ["tsrs_modulespecifiers", "modulespecifiers"],
        ["tsrs_declarations", "declarations"],
    ]) {
        const dir = path.join(root, "crates", crate, "src"),
            rows = fs
                .readdirSync(dir)
                .sort()
                .filter((f) => f.endsWith(".rs") && !f.endsWith("_test.rs"))
                .flatMap((f) => scan(path.join(dir, f), f))
                .sort((a, b) => {
                    for (const i of [0, 3, 2]) {
                        if (a[i] < b[i]) return -1;
                        if (a[i] > b[i]) return 1;
                    }
                    return 0;
                }),
            out = path.join(root, "docs/sigs", name + ".txt");
        write(
            out,
            "# rust_name | signature | origin | receiver  (generated from the Rust sources by tools/sigs-from-rust.mjs)\n" +
                rows.map((r) => r.join(" | ") + "\n").join(""),
        );
        console.log(`wrote ${rows.length} signatures to ${out}`);
    }
});
