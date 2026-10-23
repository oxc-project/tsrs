#!/usr/bin/env node
import path from "node:path";
import { ROOT, args, read, walk, main } from "../../lib/cli.mjs";
const base = path.join(ROOT, "ts-ref/tsc/testdata/tests/cases"),
    MODULE = {
        none: 0,
        commonjs: 1,
        amd: 2,
        umd: 3,
        system: 4,
        es6: 5,
        es2015: 5,
        es2020: 6,
        es2022: 7,
        esnext: 99,
        node16: 100,
        node18: 101,
        node20: 102,
        nodenext: 199,
        preserve: 200,
    },
    RESOLUTION = { classic: 1, node: 2, node10: 2, node16: 3, nodenext: 99, bundler: 100 },
    JSX = { none: 0, preserve: 1, react: 2, "react-native": 3, "react-jsx": 4, "react-jsxdev": 5 };
const bools = Object.fromEntries(
        "resolveJsonModule allowJs noDtsResolution preserveSymlinks resolvePackageJsonExports resolvePackageJsonImports allowArbitraryExtensions"
            .split(" ")
            .map((s) => [s.toLowerCase(), s]),
    ),
    lists = Object.fromEntries(
        "customConditions moduleSuffixes rootDirs typeRoots types".split(" ").map((s) => [s.toLowerCase(), s]),
    ),
    paths = Object.fromEntries("outDir declarationDir rootDir".split(" ").map((s) => [s.toLowerCase(), s]));
const absolute = (file, cwd) => {
    const input = file.startsWith("/") ? file : cwd + "/" + file;
    const parts = [];
    for (const part of input.split("/")) {
        if (!part || part === ".") continue;
        if (part === "..") parts.pop();
        else parts.push(part);
    }
    return (input.startsWith("/") ? "/" : "") + parts.join("/");
};
const first = (v) => String(v).split(",")[0].trim().toLowerCase(),
    variants = (v, table) =>
        [
            ...new Set(
                typeof v === "string"
                    ? v
                          .split(",")
                          .map((s) => table[s.trim().toLowerCase()])
                          .filter((v) => v !== undefined)
                    : [],
            ),
        ].slice(0, 3);
function option(opts, key, value, cwd, variants_) {
    const k = key.toLowerCase();
    if (k in bools && ["true", "false"].includes(first(value))) opts[bools[k]] = first(value) === "true";
    else if (k in lists) {
        let items = Array.isArray(value)
            ? value
            : String(value)
                  .split(",")
                  .map((s) => s.trim())
                  .filter(Boolean);
        if (["rootDirs", "typeRoots"].includes(lists[k])) items = items.map((s) => absolute(s, cwd));
        opts[lists[k]] = items;
    } else if (k in paths) opts[paths[k]] = absolute(value, cwd);
    else if (k === "jsx" && first(value) in JSX) opts.jsx = JSX[first(value)];
    else if (k === "paths") {
        try {
            const object = typeof value === "string" ? JSON.parse(value) : value;
            if (object && !Array.isArray(object) && typeof object === "object") {
                opts.paths = Object.entries(object).filter(([, v]) => Array.isArray(v));
                opts.pathsBasePath ??= cwd;
            }
        } catch {}
    } else if (k === "baseurl") opts.pathsBasePath = absolute(value, cwd);
    else if (k === "module") variants_.module = variants(value, MODULE);
    else if (k === "moduleresolution") variants_.moduleResolution = variants(value, RESOLUTION);
}
export function parseTest(file) {
    const content = read(file).replace(/\r\n?/g, "\n"),
        settings = Object.fromEntries(
            [...content.matchAll(/^\/\/\s*@(\w+)\s*:\s*([^\r\n]*)/gm)].map((m) => [
                m[1].toLowerCase(),
                m[2].trim().replace(/;$/, ""),
            ]),
        ),
        cwd = settings.currentdirectory ?? "/.src",
        files = {},
        links = [];
    let current = path.basename(file),
        lines = [];
    for (const line of content.split("\n")) {
        const name = line.match(/^\/\/\s*@filename\s*:\s*([^\r\n]*)/i),
            link = line.match(/^\/\/\s*@link\s*:\s*(.+?)\s*->\s*(.+?)\s*$/i);
        if (name) {
            if (lines.length || current !== path.basename(file)) files[absolute(current, cwd)] = lines.join("\n");
            current = name[1].trim();
            lines = [];
        } else if (link) links.push([link[1], link[2]]);
        else if (!/^\/\/\s*@\w+\s*:/.test(line)) lines.push(line);
    }
    files[absolute(current, cwd)] = lines.join("\n");
    const options = {},
        variantValues = {};
    for (const [k, v] of Object.entries(settings)) option(options, k, v, cwd, variantValues);
    const configs = Object.keys(files)
        .filter((f) => path.posix.basename(f) === "tsconfig.json")
        .sort((a, b) => a.length - b.length);
    if (configs.length) {
        const config = configs[0];
        try {
            const text = files[config]
                    .replace(/"(?:[^"\\]|\\.)*"|\/\/[^\n]*|\/\*[\s\S]*?\*\//g, (s) => (s.startsWith('"') ? s : ""))
                    .replace(/,(\s*[}\]])/g, "$1"),
                parsed = JSON.parse(text);
            for (const [k, v] of Object.entries(parsed.compilerOptions ?? {}))
                option(options, k, v, path.posix.dirname(config), variantValues);
            options.configFilePath = config;
        } catch {}
    }
    const outputFiles = { ...files };
    for (const [target, link] of links) outputFiles[absolute(link, cwd)] = { symlink: absolute(target, cwd) };
    const requests = [],
        seen = new Set(),
        request = (kind, name, file) => {
            for (const mode of [1, 99]) {
                const id = JSON.stringify([kind, name, file, mode]);
                if (!seen.has(id)) {
                    seen.add(id);
                    requests.push({ kind, name, file, mode });
                }
            }
        };
    for (const [file, text] of Object.entries(files))
        if (/\.(d\.)?[mc]?[jt]sx?$/.test(file)) {
            for (const pattern of [
                /\bfrom\s*['"]([^'"\n]+)['"]/g,
                /^\s*import\s*['"]([^'"\n]+)['"]/gm,
                /\bimport\s*\(\s*['"]([^'"\n]+)['"]/g,
                /\brequire\s*\(\s*['"]([^'"\n]+)['"]/g,
                /\bexport\s*\*\s*from\s*['"]([^'"\n]+)['"]/g,
                /\bmodule\s+['"]([^'"\n]+)['"]/g,
            ])
                for (const m of text.matchAll(pattern)) request("module", m[1], file);
            for (const m of text.matchAll(/\/\/\/\s*<reference\s+types\s*=\s*['"]([^'"]+)['"]/g))
                request("type", m[1], file);
        }
    for (const type of options.types ?? []) request("type", type, cwd + "/__inferred type names__.ts");
    if (!requests.length) return [];
    return (variantValues.module?.length ? variantValues.module : [null]).flatMap((module) =>
        (variantValues.moduleResolution?.length ? variantValues.moduleResolution : [null]).map((resolution) => ({
            name:
                path.relative(base, file) +
                `[module=${module === null ? "None" : module},moduleResolution=${resolution === null ? "None" : resolution}]`,
            cwd,
            caseSensitive: settings.usecasesensitivefilenames?.toLowerCase() !== "false",
            files: outputFiles,
            options: {
                ...options,
                ...(module === null ? {} : { module }),
                ...(resolution === null ? {} : { moduleResolution: resolution }),
            },
            requests: requests.slice(0, 80),
        })),
    );
}
main(() => {
    const a = args("Build resolver JSONL scenarios from the pinned corpus.", {
        corpus: base,
        "handcrafted-only": false,
    });
    let count = 0;
    if (!a["handcrafted-only"])
        for (const suite of ["compiler", "conformance"])
            for (const file of walk(path.join(a.corpus, suite), (f) => /\.(ts|tsx|js)$/.test(f)))
                if (
                    /node_modules|@moduleResolution|package\.json|@traceResolution|@paths|rootDirs|typeRoots|reference types|@types|customConditions|moduleSuffixes|@link/i.test(
                        read(file),
                    )
                )
                    for (const s of parseTest(file)) {
                        console.log(JSON.stringify(s));
                        count++;
                    }
    const handcrafted = read(new URL("./handcrafted.jsonl", import.meta.url));
    process.stdout.write(handcrafted);
    count += handcrafted.trim().split("\n").length;
    console.error(`${count} scenarios`);
});
