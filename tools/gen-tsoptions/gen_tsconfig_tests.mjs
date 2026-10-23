#!/usr/bin/env node
import { args, read, write, requireValue, main } from "../lib/cli.mjs";
const raw = (s) => {
    let hashes = "#";
    while (s.includes('"' + hashes)) hashes += "#";
    return `r${hashes}"${s}"${hashes}`;
};
function skipString(s, i) {
    const m = /^r(#*)"/.exec(s.slice(i));
    if (m) {
        const end = s.indexOf('"' + m[1], i + m[0].length);
        requireValue(end >= 0, "unterminated raw string");
        return end + m[1].length + 1;
    }
    const quote = s[i];
    let j = i + 1;
    while (j < s.length && s[j] !== quote) {
        if (quote === '"' && s[j] === "\\") j++;
        j++;
    }
    requireValue(j < s.length, "unterminated string");
    return j + 1;
}
function block(s, start) {
    let depth = 0;
    for (let i = start; i < s.length; i++) {
        if (s[i] === '"' || s[i] === "`" || /^r#*"/.test(s.slice(i))) i = skipString(s, i) - 1;
        else if (s[i] === "{") depth++;
        else if (s[i] === "}" && --depth === 0) return i + 1;
    }
    throw new Error("unclosed block");
}
function strings(s) {
    let out = "";
    for (let i = 0; i < s.length; ) {
        if (s[i] === "`" || s[i] === '"') {
            const end = skipString(s, i);
            out += s[i] === "`" ? raw(s.slice(i + 1, end - 1)) : s.slice(i, end);
            i = end;
        } else out += s[i++];
    }
    return out;
}
main(() => {
    const a = args("Convert Go tsconfig parsing tables to a Rust generated region. <go-file> <rust-file>");
    requireValue(a._.length === 2, "Go and Rust files are required");
    const go = read(a._[0]),
        out = [];
    for (const name of [
        "tsconfigWithExtends",
        "tsconfigWithoutConfigDir",
        "tsconfigWithConfigDir",
        "tsconfigWithExtendsAndConfigDir",
    ]) {
        const m = new RegExp("^var " + name + " = `([\\s\\S]*?)`", "m").exec(go);
        requireValue(m, `missing ${name}`);
        out.push(`const ${name.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toUpperCase()}: &str = ${raw(m[1])};\n`);
    }
    const head = /^var parseJsonConfigFileTests = \[\]parseJsonConfigTestCase\{/m.exec(go);
    requireValue(head, "missing parseJsonConfigFileTests");
    const start = head.index + head[0].length - 1;
    let s = strings(go.slice(start + 1, block(go, start) - 1)),
        result = "",
        cursor = 0;
    for (;;) {
        const j = s.indexOf("map[string]string{", cursor);
        if (j < 0) {
            result += s.slice(cursor);
            break;
        }
        result += s.slice(cursor, j);
        const k = j + "map[string]string{".length,
            end = block(s, k - 1),
            body = s.slice(k, end - 1),
            entries = [];
        let pos = 0;
        for (;;) {
            const m = /^\s*("(?:[^"\\]|\\.)*")\s*:\s*/.exec(body.slice(pos));
            if (!m) break;
            const key = m[1],
                vpos = pos + m[0].length;
            let vend;
            if (body[vpos] === '"' || /^r#*"/.test(body.slice(vpos))) vend = skipString(body, vpos);
            else {
                vend = vpos;
                while (vend < body.length && !",\n".includes(body[vend])) vend++;
            }
            const value = body
                .slice(vpos, vend)
                .trim()
                .replace(
                    /\btsconfig(\w+)/g,
                    (_, n) => "TSCONFIG_" + n.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toUpperCase(),
                );
            entries.push(`(${key}, ${value})`);
            pos = vend;
            const comma = /^\s*,/.exec(body.slice(pos));
            if (comma) pos += comma[0].length;
        }
        result += "&[" + entries.join(", ") + "]";
        cursor = end;
    }
    s = result.replaceAll("[]testConfig{", "vec![");
    for (const [from, to] of Object.entries({
        includeCompilerOptions: "include_compiler_options",
        jsonText: "json_text",
        configFileName: "config_file_name",
        basePath: "base_path",
        allFileList: "all_file_list",
    }))
        s = s.replace(new RegExp(`\\b${from}:`, "g"), to + ":");
    const rules = {
        "\t{": ["\tParseJsonConfigTestCase {"],
        "\t},": ["\t\t..Default::default()", "\t},"],
        "\t\tinput: vec![{": ["\t\tinput: vec![TestConfig {"],
        "\t\t}, {": ["\t\t\t..Default::default()", "\t\t}, TestConfig {"],
        "\t\t}},": ["\t\t\t..Default::default()", "\t\t}],"],
        "\t\t\t{": ["\t\t\tTestConfig {"],
        "\t\t\t},": ["\t\t\t\t..Default::default()", "\t\t\t},"],
        "\t\t},": ["\t\t],"],
    };
    const table = s
        .split("\n")
        .flatMap((line) => rules[line] ?? [line])
        .join("\n");
    out.push(`\nfn parse_json_config_file_tests() -> Vec<ParseJsonConfigTestCase> {\n    vec![\n${table}\n    ]\n}\n`);
    const target = read(a._[1]),
        begin = target.indexOf("// BEGIN GENERATED\n"),
        end = target.indexOf("// END GENERATED");
    requireValue(begin >= 0 && end > begin, "missing generated markers");
    write(a._[1], target.slice(0, begin + "// BEGIN GENERATED\n".length) + out.join("") + target.slice(end));
});
