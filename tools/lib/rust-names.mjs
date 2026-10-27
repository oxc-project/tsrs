// Rust constant names are independent of upstream identifiers and serialized labels.
export const screamingSnakeCase = (name) =>
    name
        .replace(/JSDoc/g, "Jsdoc")
        .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
        .replace(/([A-Z]+)([A-Z][a-z])/g, "$1_$2")
        .toUpperCase();

export function constantNames(names) {
    const result = new Map(), seen = new Map();
    for (const name of names) {
        const rust = screamingSnakeCase(name);
        if (seen.has(rust)) throw new Error(`Rust constant name collision: ${seen.get(rust)} and ${name} -> ${rust}`);
        result.set(name, rust);
        seen.set(rust, name);
    }
    return result;
}

// Apply the generators' explicit declaration policies without changing their schema or storage layout.
// The input is generated Rust: declarations start on their own line and inherent impls have named types.
export function rustSurface(source, rows) {
    const policy = new Map();
    for (const [visibility, kind, owner, names] of rows)
        for (const name of names.split(" ")) policy.set(`${owner}:${kind}:${name}`, visibility);
    // Mask literals and comments while preserving offsets and newlines for declaration/body boundaries.
    const masked = source.replace(
        /\/\/[^\n]*|\/\*[\s\S]*?\*\/|r(#{0,})"[\s\S]*?"\1|"(?:\\.|[^"\\])*"|'(?:\\(?:u\{[\da-fA-F]+\}|x[\da-fA-F]{2}|.)|[^'\\\n])'/g,
        (text) => text.replace(/[^\n]/g, " "),
    );
    const stack = [], ends = new Map();
    for (let i = 0; i < masked.length; i++) {
        if (masked[i] === "{") stack.push(i);
        else if (masked[i] === "}") {
            const start = stack.pop();
            if (start === undefined) throw new Error("Unbalanced generated Rust body");
            ends.set(start, i + 1);
        }
    }
    if (stack.length) throw new Error("Unclosed generated Rust body");
    const scopes = [];
    for (const m of masked.matchAll(/^(?:pub(?:\([^)]*\))? )?(?:struct|enum) (\w+)[^{;\n]*\{|^impl ([^{\n]+)\{/gm)) {
        const start = m.index + m[0].length - 1;
        const owner = m[1] ?? m[2].trim().split(" for ").at(-1);
        scopes.push({ start, end: ends.get(start), owner });
    }
    const edits = [], found = new Set();
    const declaration = /^([ \t]*)(pub(?:\([^)]*\))? )(?:(?:(?:const|unsafe|async) )*(fn) (\w+)|(struct|enum|type|const|static) (\w+)|(\w+)\s*:)/gm;
    for (const m of masked.matchAll(declaration)) {
        const enclosing = scopes.filter((s) => s.start < m.index && m.index < s.end).at(-1);
        const owner = enclosing?.owner ?? "";
        const kind = m[3] ?? m[5] ?? "field", name = m[4] ?? m[6] ?? m[7];
        const key = `${owner}:${kind}:${name}`;
        if (!policy.has(key)) continue;
        found.add(key);
        const visibility = policy.get(key);
        let start = m.index + m[1].length, end = start + m[2].length;
        if (visibility === "omit") {
            if (kind === "fn") {
                const body = masked.indexOf("{", end);
                end = ends.get(body);
                if (end === undefined) throw new Error(`Missing generated function body: ${key}`);
            } else if (kind === "type" || kind === "const" || kind === "static") end = masked.indexOf(";", end) + 1;
            else throw new Error(`Cannot omit schema declaration: ${key}`);
            start = m.index;
            // Include the function's generated documentation and attributes, but retain section comments.
            while (start > 0) {
                const previous = source.lastIndexOf("\n", start - 2) + 1;
                const line = source.slice(previous, start).trim();
                if (!line.startsWith("///") && !/^#\[.*\]$/.test(line)) break;
                start = previous;
            }
            if (source[end] === "\n") end++;
            edits.push({ start, end, text: "" });
        } else edits.push({ start, end, text: visibility ? `${visibility} ` : "" });
    }
    for (const m of masked.matchAll(/^(?:pub(?:\([^)]*\))? )?struct (\w+)\((pub(?:\([^)]*\))? )/gm)) {
        const key = `${m[1]}:field:0`;
        if (!policy.has(key)) continue;
        found.add(key);
        const visibility = policy.get(key);
        if (visibility === "omit") throw new Error(`Cannot omit schema field: ${key}`);
        const end = m.index + m[0].length;
        edits.push({ start: end - m[2].length, end, text: visibility ? `${visibility} ` : "" });
    }
    const missing = [...policy.keys()].filter((key) => !found.has(key));
    if (missing.length) throw new Error(`Stale generated declaration policy: ${missing.join(", ")}`);
    for (const edit of edits.sort((a, b) => b.start - a.start))
        source = source.slice(0, edit.start) + edit.text + source.slice(edit.end);
    return source.replace(/^impl \w+ \{\s*\}\n/gm, "").replace(/\n(?:[ \t]*\n){2,}/g, "\n\n");
}
