#!/usr/bin/env node
// Bundle declarations with rolldown + rolldown-plugin-dts, as tsdown does for a `dts`-only build. The harness
// (dts.mjs) runs this in a fresh process per sample. Tools resolve from --tools, the prepared install of
// bench/dts/package.json, never from the repository's own node_modules.
import { createRequire } from "node:module";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: {
        tools: { type: "string" },
        tsconfig: { type: "string" },
        "out-dir": { type: "string" },
        generator: { type: "string" },
        tsgo: { type: "string" },
    },
});
for (const key of ["tools", "tsconfig", "out-dir", "generator"])
    if (!values[key]) throw new Error(`--${key} is required`);
if (!["tsc", "tsgo"].includes(values.generator)) throw new Error("--generator must be tsc or tsgo");
if (values.generator === "tsgo" && !values.tsgo) throw new Error("--generator tsgo needs --tsgo <TypeScript 7 executable>");
if (positionals.length === 0) throw new Error("usage: rolldown.mjs [options] <name.d.ts>=<entry>...");

const require = createRequire(path.join(path.resolve(values.tools), "package.json"));
const load = (specifier) => import(pathToFileURL(require.resolve(specifier)).href);
const { rolldown } = await load("rolldown");
const { dts } = await load("rolldown-plugin-dts");

const input = {};
for (const argument of positionals) {
    const [name, entry] = argument.split("=");
    if (!name.endsWith(".d.ts") || !entry) throw new Error(`expected <name.d.ts>=<entry>, got ${argument}`);
    input[name.slice(0, -".d.ts".length)] = path.resolve(entry);
}
const tsconfig = path.resolve(values.tsconfig);
const started = performance.now();
const bundle = await rolldown({
    input,
    cwd: path.dirname(tsconfig),
    platform: "neutral",
    external: [/^node:/],
    logLevel: "warn",
    plugins: [
        dts({
            tsconfig,
            generator: values.generator,
            emitDtsOnly: true,
            ...(values.generator === "tsgo" ? { tsgo: { path: path.resolve(values.tsgo) } } : {}),
        }),
    ],
});
const { output } = await bundle.write({ dir: path.resolve(values["out-dir"]), format: "es" });
await bundle.close();
const build_ms = performance.now() - started;
const files = output.filter((item) => item.type === "chunk" || item.type === "asset");
const bytes = files.reduce((sum, item) => sum + Buffer.byteLength(item.type === "chunk" ? item.code : item.source), 0);
console.log(JSON.stringify({ build_ms, files: files.length, bytes }));
