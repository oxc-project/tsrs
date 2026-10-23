#!/usr/bin/env node
import { args, read, write, requireValue, main } from "../lib/cli.mjs";
import { screamingSnakeCase as scream } from "../lib/rust-names.mjs";
const snake = (s) =>
    s
        .replace(/([A-Z]+)([A-Z][a-z])/g, "$1_$2")
        .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
        .toLowerCase();
const moved = new Set(
    "affects_declaration_path affects_program_structure affects_semantic_diagnostics affects_build_info affects_bind_diagnostics affects_source_file affects_module_resolution affects_emit allow_js_flag strict_flag transpile_option_value allow_config_dir_template_substitution".split(
        " ",
    ),
);
const tristate = (s) =>
    ({ "core.TSUnknown": "Tristate::Unknown", "core.TSTrue": "Tristate::True", "core.TSFalse": "Tristate::False" })[s];
function defaultValue(s) {
    if (s === "true" || s === "false") return `Bool(${s})`;
    if (s === "nil") return "None";
    if (s.startsWith('"')) return `Str(${s})`;
    if (/^-?\d+$/.test(s)) return `Int(${s})`;
    if (s.startsWith("diagnostics.")) return `Message(&diagnostics::${scream(s.slice(12))})`;
    if (s.startsWith("core.TS")) return `Tristate(${tristate(s)})`;
    if (s.startsWith("core.ScriptTarget")) return `ScriptTarget(ScriptTarget::${scream(s.slice(17))})`;
    throw new Error(`unknown default value ${s}`);
}
function field(key, value) {
    if (key === "PathKind") return null;
    if (key === "Name") return `name: ${value}`;
    if (key === "ShortName") return `short_name: ${value}`;
    if (key === "Kind") {
        const kind = value.replace("CommandLineOptionType", "").replaceAll('"', "");
        return `kind: CommandLineOptionKind::${kind[0].toUpperCase()}${kind.slice(1)}`;
    }
    if (key === "Category" || key === "Description") {
        requireValue(value.startsWith("diagnostics."), `unknown diagnostic ${value}`);
        return `${snake(key)}: Some(&diagnostics::${scream(value.slice(12))})`;
    }
    if (key === "DefaultValueDescription")
        return `default_value_description: DefaultValueDescription::${defaultValue(value)}`;
    if (key === "transpileOptionValue") return `transpile_option_value: ${tristate(value)}`;
    if (key === "extraValidation") return `extra_validation: ExtraValidation::${value.slice(15)}`;
    if (key === "minValue") return `min_value: ${value}`;
    if (value === "true" || value === "false")
        return `${key === "IsTSConfigOnly" ? "is_tsconfig_only" : snake(key)}: ${value}`;
    throw new Error(`unknown field ${key}: ${value}`);
}
main(() => {
    const a = args("Convert Go option declarations into Rust // BEGIN GENERATED regions. <go-file> <rust-file>", {
        vars: null,
    });
    requireValue(a._.length === 2, "Go and Rust file paths are required");
    const go = read(a._[0]),
        target = read(a._[1]),
        previous = new Map(),
        selected = a.vars ? new Set(a.vars.split(",")) : null;
    for (const m of target
        .split("// BEGIN GENERATED\n")[1]
        .split("// END GENERATED")[0]
        .matchAll(/CommandLineOption \{\n(.*?)\n\s*\.\.CommandLineOption::DEFAULT\n\s*\}/gs)) {
        const name = m[1].match(/name: "([^"]+)"/),
            short = m[1].match(/short_name: "([^"]+)"/);
        if (!name) continue;
        const fields = {};
        for (const l of m[1].split("\n")) {
            const f = l.match(/^\s*(\w+):\s*(.*),\s*$/);
            if (f && moved.has(f[1])) fields[f[1]] = f[2];
        }
        previous.set(JSON.stringify([name[1], short?.[1] ?? ""]), fields);
    }
    function literal(body, indent) {
        const fields = [];
        for (const l of body.split("\n")) {
            const line = l.replace(/\/\/.*$/, "").trim();
            if (!line) continue;
            const m = line.match(/^(\w+):\s*(.+?),?$/);
            requireValue(m, `cannot parse line: ${line}`);
            const f = field(m[1], m[2]);
            if (f) fields.push(f);
        }
        const name = body.match(/\bName:\s*"([^"]+)"/),
            short = body.match(/\bShortName:\s*"([^"]+)"/),
            present = new Set(fields.map((s) => s.split(":")[0]));
        if (name)
            for (const [k, v] of Object.entries(previous.get(JSON.stringify([name[1], short?.[1] ?? ""])) ?? {}))
                if (!present.has(k)) fields.push(`${k}: ${v}`);
        const pad = " ".repeat(indent + 4);
        return `CommandLineOption {\n${fields.map((f) => `${pad}${f},\n`).join("")}${pad}..CommandLineOption::DEFAULT\n${" ".repeat(indent)}}`;
    }
    const out = [];
    for (const m of go.matchAll(/^var (\w+) = CommandLineOption\{\n(.*?)\n\}/gms)) {
        if (selected && !selected.has(m[1])) continue;
        out.push(
            `${/^[A-Z]/.test(m[1]) ? "pub" : "pub(crate)"} static ${scream(m[1])}: CommandLineOption = ${literal(m[2], 0)};\n`,
        );
    }
    for (const m of go.matchAll(/^var (\w+) = \[\]\*CommandLineOption\{\n(.*?)\n\}\n/gms)) {
        if (selected && !selected.has(m[1])) continue;
        const name = scream(m[1]),
            vis = /^[A-Z]/.test(m[1]) ? "pub" : "pub(crate)",
            items = [];
        let cur = null;
        for (const line of m[2].split("\n")) {
            const s = line.replace(/\/\/.*$/, "").trim();
            if (cur === null) {
                if (s === "{") cur = [];
                else if (s.startsWith("&")) items.push(["ref", s.slice(1).replace(/,$/, "")]);
                else requireValue(!s, `unexpected top-level line: ${line}`);
            } else if (s === "}," || s === "}") {
                items.push(["lit", cur.join("\n")]);
                cur = null;
            } else cur.push(line);
        }
        const lits = items.filter(([k]) => k === "lit"),
            arr = `${name}_ITEMS`;
        let li = 0;
        out.push(
            `static ${arr}: [CommandLineOption; ${lits.length}] = [\n${lits.map(([, b]) => `    ${literal(b, 4)},\n`).join("")}];\n\n${vis} static ${name}: LazyLock<Vec<&'static CommandLineOption>> = LazyLock::new(|| {\n    vec![\n${items.map(([k, v]) => `        &${k === "ref" ? scream(v) : `${arr}[${li++}]`},\n`).join("")}    ]\n});\n`,
        );
    }
    const start = target.indexOf("// BEGIN GENERATED\n") + "// BEGIN GENERATED\n".length,
        end = target.indexOf("// END GENERATED");
    write(a._[1], target.slice(0, start) + out.join("\n") + target.slice(end));
});
