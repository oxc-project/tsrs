#!/usr/bin/env node
// Print each root module's exports with their meanings (value, type, namespace) as JSON, using the TypeScript
// compiler API from --tools. dts.mjs compares a bundle's entries with the source entries this way.
//
//   exports.mjs --tools <dir> --tsconfig <file> <name>=<root file>...
import { createRequire } from "node:module";
import path from "node:path";
import { parseArgs } from "node:util";

const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: { tools: { type: "string" }, tsconfig: { type: "string" } },
});
if (!values.tools || !values.tsconfig || positionals.length === 0)
    throw new Error("usage: exports.mjs --tools <dir> --tsconfig <file> <name>=<root file>...");
const ts = createRequire(path.join(path.resolve(values.tools), "package.json"))("typescript");

const config = ts.getParsedCommandLineOfConfigFile(path.resolve(values.tsconfig), {}, {
    ...ts.sys,
    onUnRecoverableConfigFileDiagnostic: (diagnostic) => {
        throw new Error(ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"));
    },
});
const roots = positionals.map((argument) => {
    const [name, file] = argument.split("=");
    return { name, file: path.resolve(file) };
});
const program = ts.createProgram({
    rootNames: roots.map((root) => root.file),
    options: { ...config.options, noEmit: true },
});
const checker = program.getTypeChecker();
const meaning = (symbol) => {
    const flags = (symbol.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(symbol) : symbol).flags;
    return [
        flags & ts.SymbolFlags.Value ? "value" : "",
        flags & ts.SymbolFlags.Type ? "type" : "",
        flags & ts.SymbolFlags.Namespace ? "namespace" : "",
    ]
        .filter(Boolean)
        .join("+");
};
const result = {};
for (const { name, file } of roots) {
    const source = program.getSourceFile(file);
    const symbol = source && checker.getSymbolAtLocation(source);
    if (!symbol) throw new Error(`${file} is not a module`);
    result[name] = Object.fromEntries(
        checker
            .getExportsOfModule(symbol)
            .map((item) => [item.escapedName, meaning(item)])
            .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
    );
}
console.log(JSON.stringify(result));
