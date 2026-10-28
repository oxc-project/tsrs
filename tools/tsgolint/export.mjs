#!/usr/bin/env node
// Export evaluated baseline cases from an isolated pinned tsgolint checkout.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { parseArgs } from "node:util";
import { main, writeJSON } from "../lib/cli.mjs";

const REFERENCE = "4ebf80fd2b166cba4890c6896220fec4a1462b29";
const COMPILER = "2bd066d87f5bafd315be9f40889d0a60b9e58e0b";

const HOOK = `
func exportBaselineSuite(rootDir, tsconfigPath string, t *testing.T, r *rule.Rule, valid []ValidTestCase, invalid []InvalidTestCase) {
    dir := os.Getenv("TSRS_BASELINE_EXPORT")
    if dir == "" { return }
    data, err := json.Marshal(struct {
        Name string
        TSConfig string
        Valid []ValidTestCase
        Invalid []InvalidTestCase
    }{r.Name, tsconfigPath, valid, invalid}, jsontext.WithIndent("  "))
    if err != nil { t.Fatal(err) }
    if err := os.MkdirAll(filepath.Join(dir, r.Name), 0755); err != nil { t.Fatal(err) }
    name := strings.ReplaceAll(t.Name(), "/", "_")
    if err := os.WriteFile(filepath.Join(dir, r.Name, name+".cases.json"), data, 0644); err != nil { t.Fatal(err) }
}

func exportBaselineResult(t *testing.T, r *rule.Rule, rootDir string, diagnostics []rule.RuleDiagnostic) {
    dir := os.Getenv("TSRS_BASELINE_EXPORT")
    if dir == "" { return }
    type fix struct { Text string \`json:"text"\`; Range map[string]int \`json:"range"\` }
    type message struct { Id string \`json:"id"\`; Description string \`json:"description"\`; Help string \`json:"help,omitempty"\` }
    type suggestion struct { Message message \`json:"message"\`; Fixes []fix \`json:"fixes"\` }
    toMessage := func(m rule.RuleMessage) message { return message{m.Id, m.Description, m.Help} }
    toFixes := func(fixes []rule.RuleFix) []fix {
        out := make([]fix, len(fixes))
        for i, f := range fixes { out[i] = fix{f.Text, map[string]int{"pos": f.Range.Pos(), "end": f.Range.End()}} }
        return out
    }
    out := make([]map[string]any, len(diagnostics))
    for i, d := range diagnostics {
        record := map[string]any{
            "kind": 0, "rule": r.Name, "message": toMessage(d.Message),
            "file_path": strings.TrimPrefix(d.SourceFile.FileName(), rootDir+"/"),
            "range": map[string]int{"pos": d.Range.Pos(), "end": d.Range.End()},
        }
        if len(d.Fixes()) > 0 { record["fixes"] = toFixes(d.Fixes()) }
        if len(d.GetSuggestions()) > 0 {
            suggestions := make([]suggestion, len(d.GetSuggestions()))
            for j, s := range d.GetSuggestions() { suggestions[j] = suggestion{toMessage(s.Message), toFixes(s.Fixes())} }
            record["suggestions"] = suggestions
        }
        if len(d.LabeledRanges) > 0 {
            labels := make([]map[string]any, len(d.LabeledRanges))
            for j, label := range d.LabeledRanges {
                labels[j] = map[string]any{"label": label.Label, "range": map[string]int{"pos": label.Range.Pos(), "end": label.Range.End()}}
            }
            record["labeled_ranges"] = labels
        }
        out[i] = record
    }
    data, err := json.Marshal(out, jsontext.WithIndent("  "))
    if err != nil { t.Fatal(err) }
    name := strings.ReplaceAll(t.Name(), "/", "_")
    if err := os.WriteFile(filepath.Join(dir, r.Name, name+".diagnostics.json"), data, 0644); err != nil { t.Fatal(err) }
}
`;

const USAGE = "usage: export.mjs [-h] --rule RULE reference output";
const HELP = `${USAGE}

Export evaluated baseline cases from an isolated pinned tsgolint checkout.

positional arguments:
  reference    Disposable checkout; this tool patches its rule tester
  output

options:
  -h, --help   show this help message and exit
  --rule RULE  Rule directory name, e.g. await_thenable, or all`;

function usageError(message) {
    console.error(`${USAGE}\nexport.mjs: error: ${message}`);
    process.exit(2);
}

function run(argv, options = {}) {
    const { check = true, ...rest } = options;
    const result = spawnSync(argv[0], argv.slice(1), { stdio: "inherit", ...rest });
    if (result.error) throw result.error;
    if (check && result.status !== 0) throw new Error(`${argv.join(" ")} exited ${result.status ?? result.signal}`);
    return result;
}

const output = (argv, options = {}) =>
    run(argv, { stdio: ["ignore", "pipe", "inherit"], encoding: "utf8", ...options }).stdout;
const gitHead = (checkout) => output(["git", "-C", checkout, "rev-parse", "HEAD"]).trim();
const count = (text, part) => text.split(part).length - 1;
const copyTree = (source, target) => fs.cpSync(source, target, { recursive: true, dereference: true });
const isDirectory = (file) => fs.existsSync(file) && fs.statSync(file).isDirectory();
const sorted = (names) => names.sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));

function assertOnce(text, part) {
    if (count(text, part) !== 1) throw new Error(`expected exactly one ${JSON.stringify(part)}`);
}

function replaceOnce(text, part, replacement) {
    return text.replace(part, () => replacement);
}

function writeBaselines(staging, exported) {
    for (const rule of sorted(fs.readdirSync(staging))) {
        const folder = path.join(staging, rule);
        const pending = new Set(fs.readdirSync(folder));
        const take = (name) => {
            pending.delete(name);
            return JSON.parse(fs.readFileSync(path.join(folder, name), "utf8"));
        };
        const suites = {};
        for (const name of sorted([...pending].filter((entry) => entry.endsWith(".cases.json")))) {
            const test = name.slice(0, -".cases.json".length);
            const { Name, ...suite } = take(name);
            if (Name !== rule) throw new Error(`${rule}/${name} exports rule ${Name}`);
            for (const group of ["Valid", "Invalid"])
                for (const [index, testCase] of suite[group].entries()) {
                    const diagnostics = `${test}_${group.toLowerCase()}-${index}.diagnostics.json`;
                    if (pending.has(diagnostics)) testCase.Diagnostics = take(diagnostics);
                }
            suites[test] = suite;
        }
        if (pending.size) throw new Error(`${rule} has unmatched exports: ${sorted([...pending]).join(", ")}`);
        writeJSON(path.join(exported, rule, "baseline.json"), { Name: rule, Suites: suites });
    }
}

main(() => {
    let parsed;
    try {
        parsed = parseArgs({
            options: { help: { type: "boolean", short: "h" }, rule: { type: "string" } },
            allowPositionals: true,
        });
    } catch (error) {
        usageError(error.message);
    }
    const { values, positionals } = parsed;
    if (values.help) {
        console.log(HELP);
        return;
    }
    const missing = [...(positionals.length < 1 ? ["reference"] : []), ...(positionals.length < 2 ? ["output"] : [])];
    if (values.rule === undefined) missing.push("--rule");
    if (missing.length) usageError(`the following arguments are required: ${missing.join(", ")}`);
    if (positionals.length > 2) usageError(`unrecognized arguments: ${positionals.slice(2).join(" ")}`);
    const reference = fs.realpathSync(positionals[0]);
    const exported = path.resolve(positionals[1]);
    if (gitHead(reference) !== REFERENCE || gitHead(path.join(reference, "typescript-go")) !== COMPILER)
        usageError("reference revisions do not match the pinned baseline");
    const compiler = path.join(reference, "typescript-go");
    const patches = path.join(reference, "patches");
    const patchFiles = isDirectory(patches)
        ? sorted(fs.readdirSync(patches).filter((name) => name.endsWith(".patch")))
        : [];
    for (const name of patchFiles) {
        const patch = path.join(patches, name);
        const applied =
            run(["git", "-C", compiler, "apply", "--reverse", "--check", patch], { check: false, stdio: "ignore" })
                .status === 0;
        if (!applied) run(["git", "-C", compiler, "apply", patch]);
    }
    const collections = path.join(reference, "internal/collections");
    fs.mkdirSync(collections, { recursive: true });
    for (const name of fs.readdirSync(path.join(compiler, "internal/collections"))) {
        const source = path.join(compiler, "internal/collections", name);
        if (name.endsWith(".go") && !name.endsWith("_test.go") && fs.statSync(source).isFile())
            fs.copyFileSync(source, path.join(collections, name));
    }
    const tester = path.join(reference, "internal/rule_tester/rule_tester.go");
    const original = output(["git", "-C", reference, "show", "HEAD:internal/rule_tester/rule_tester.go"]);
    let patched = replaceOnce(
        original,
        "import (",
        'import (\n "github.com/go-json-experiment/json/jsontext"\n "os"\n "path/filepath"',
    );
    const signature = "invalidTestCases []InvalidTestCase) {";
    assertOnce(patched, signature);
    patched = replaceOnce(
        patched,
        signature,
        signature + "\n exportBaselineSuite(rootDir, tsconfigPath, t, r, validTestCases, invalidTestCases)",
    );
    const validCapture =
        "\t\t\tdiagnostics := runLinter(t, testCase.Code, testCase.FileName, testCase.Options, testCase.TSConfig, testCase.Tsx, testCase.Files)";
    assertOnce(patched, validCapture);
    patched = replaceOnce(
        patched,
        validCapture,
        validCapture + "\n\t\t\texportBaselineResult(t, r, rootDir, diagnostics)",
    );
    const initialCapture = "\t\t\t\t\tinitialDiagnostics = diagnostics";
    assertOnce(patched, initialCapture);
    patched = replaceOnce(
        patched,
        initialCapture,
        initialCapture + "\n\t\t\t\t\texportBaselineResult(t, r, rootDir, diagnostics)",
    );
    fs.writeFileSync(tester, patched + HOOK);
    run(["gofmt", "-w", tester]);
    fs.mkdirSync(exported, { recursive: true });
    const staging = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-lint-export-"));
    try {
        const env = { ...process.env, TSRS_BASELINE_EXPORT: staging, UPDATE_SNAPS: "false" };
        const pkg = values.rule === "all" ? "./internal/rules/..." : "./internal/rules/" + values.rule;
        run(["go", "test", "-p", "2", "-parallel", "1", "-count=1", pkg], { cwd: reference, env });
        writeBaselines(staging, exported);
    } finally {
        fs.rmSync(staging, { recursive: true, force: true });
    }
    copyTree(path.join(reference, "internal/rules/fixtures"), path.join(exported, "fixtures"));
    for (const [name, version] of [
        ["@types/node", "24.13.2"],
        ["undici-types", "7.18.2"],
    ]) {
        let dependency = path.join(reference, "node_modules", name);
        if (!fs.existsSync(dependency) && name === "undici-types")
            dependency = path.join(
                path.dirname(path.dirname(fs.realpathSync(path.join(reference, "node_modules/@types/node")))),
                name,
            );
        if (!fs.existsSync(dependency)) usageError(`missing pinned dependency ${name}`);
        if (JSON.parse(fs.readFileSync(path.join(dependency, "package.json"), "utf8")).version !== version)
            usageError(`${name} must match pinned version ${version}`);
        copyTree(dependency, path.join(exported, "dependencies", name));
    }
    const rulesDirectory = path.join(reference, "internal/rules");
    const rules =
        values.rule === "all"
            ? sorted(fs.readdirSync(rulesDirectory)).map((name) => path.join(rulesDirectory, name))
            : [path.join(rulesDirectory, values.rule)];
    const stamp = `tsgolint ${REFERENCE}\ntypescript-go ${COMPILER}\n`;
    for (const rule of rules) {
        const tests = isDirectory(rule) ? fs.readdirSync(rule).filter((name) => name.endsWith("test.go")) : [];
        if (!tests.length) continue;
        const provenance = path.join(exported, path.basename(rule).replaceAll("_", "-"), "upstream");
        fs.mkdirSync(provenance, { recursive: true });
        for (const name of tests) fs.copyFileSync(path.join(rule, name), path.join(provenance, name));
        fs.writeFileSync(path.join(path.dirname(provenance), "REFERENCE"), stamp);
    }
    fs.writeFileSync(path.join(exported, "REFERENCE"), stamp);
});
