#!/usr/bin/env python3
"""Export evaluated baseline cases from an isolated pinned tsgolint checkout."""

import argparse
import json
import pathlib
import shutil
import subprocess

REFERENCE = "8897346fff24a9bd0c719099dc1af8e7200572c1"
COMPILER = "2bd066d87f5bafd315be9f40889d0a60b9e58e0b"

HOOK = r'''
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
    type fix struct { Text string `json:"text"`; Range map[string]int `json:"range"` }
    type message struct { Id string `json:"id"`; Description string `json:"description"`; Help string `json:"help,omitempty"` }
    type suggestion struct { Message message `json:"message"`; Fixes []fix `json:"fixes"` }
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
'''


def git_head(path):
    return subprocess.check_output(["git", "-C", str(path), "rev-parse", "HEAD"], text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=pathlib.Path, help="Disposable checkout; this tool patches its rule tester")
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--rule", required=True, help="Rule directory name, e.g. await_thenable, or all")
    args = parser.parse_args()
    reference = args.reference.resolve()
    output = args.output.resolve()
    if git_head(reference) != REFERENCE or git_head(reference / "typescript-go") != COMPILER:
        parser.error("reference revisions do not match the pinned baseline")
    compiler = reference / "typescript-go"
    for patch in sorted((reference / "patches").glob("*.patch")):
        applied = subprocess.run(["git", "-C", str(compiler), "apply", "--reverse", "--check", str(patch)], capture_output=True).returncode == 0
        if not applied:
            subprocess.run(["git", "-C", str(compiler), "apply", str(patch)], check=True)
    collections = reference / "internal/collections"
    collections.mkdir(exist_ok=True)
    for source in (compiler / "internal/collections").glob("*.go"):
        if not source.name.endswith("_test.go"):
            shutil.copy2(source, collections / source.name)
    tester = reference / "internal/rule_tester/rule_tester.go"
    original = subprocess.check_output(["git", "-C", str(reference), "show", "HEAD:internal/rule_tester/rule_tester.go"], text=True)
    patched = original.replace('import (', 'import (\n "github.com/go-json-experiment/json/jsontext"\n "os"\n "path/filepath"', 1)
    signature = "invalidTestCases []InvalidTestCase) {"
    assert patched.count(signature) == 1
    patched = patched.replace(signature, signature + "\n exportBaselineSuite(rootDir, tsconfigPath, t, r, validTestCases, invalidTestCases)", 1)
    valid_capture = "\t\t\tdiagnostics := runLinter(t, testCase.Code, testCase.FileName, testCase.Options, testCase.TSConfig, testCase.Tsx, testCase.Files)"
    assert patched.count(valid_capture) == 1
    patched = patched.replace(valid_capture, valid_capture + "\n\t\t\texportBaselineResult(t, r, rootDir, diagnostics)", 1)
    initial_capture = "\t\t\t\t\tinitialDiagnostics = diagnostics"
    assert patched.count(initial_capture) == 1
    patched = patched.replace(initial_capture, initial_capture + "\n\t\t\t\t\texportBaselineResult(t, r, rootDir, diagnostics)", 1)
    tester.write_text(patched + HOOK)
    subprocess.run(["gofmt", "-w", str(tester)], check=True)
    output.mkdir(parents=True, exist_ok=True)
    import os
    env = dict(os.environ, TSRS_BASELINE_EXPORT=str(output), UPDATE_SNAPS="false")
    package = "./internal/rules/..." if args.rule == "all" else "./internal/rules/" + args.rule
    subprocess.run(["go", "test", "-p", "2", "-parallel", "1", "-count=1", package], cwd=reference, env=env, check=True)
    shutil.copytree(reference / "internal/rules/fixtures", output / "fixtures", dirs_exist_ok=True)
    for name, version in [("@types/node", "24.13.2"), ("undici-types", "7.18.2")]:
        dependency = reference / "node_modules" / name
        if not dependency.exists() and name == "undici-types":
            dependency = (reference / "node_modules/@types/node").resolve().parent.parent / name
        if not dependency.exists():
            parser.error(f"missing pinned dependency {name}")
        if json.loads((dependency / "package.json").read_text())["version"] != version:
            parser.error(f"{name} must match pinned version {version}")
        shutil.copytree(dependency, output / "dependencies" / name, dirs_exist_ok=True)
    rules = sorted((reference / "internal/rules").iterdir()) if args.rule == "all" else [reference / "internal/rules" / args.rule]
    for rule in rules:
        if not list(rule.glob("*test.go")):
            continue
        provenance = output / rule.name.replace("_", "-") / "upstream"
        provenance.mkdir(parents=True, exist_ok=True)
        for source in rule.glob("*test.go"):
            shutil.copy2(source, provenance / source.name)
        (provenance.parent / "REFERENCE").write_text(f"tsgolint {REFERENCE}\ntypescript-go {COMPILER}\n")
    (output / "REFERENCE").write_text(f"tsgolint {REFERENCE}\ntypescript-go {COMPILER}\n")


if __name__ == "__main__":
    main()
