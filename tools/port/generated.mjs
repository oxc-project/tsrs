#!/usr/bin/env node
// Generated regions omitted from tools/gen-check.sh. New API methods require explicit coverage classification.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { referencePin, sourceVersion } from "../lib/typescript-reference.mjs";
const root = process.cwd(),
  write = process.argv[2] === "--write";
if (process.argv[2] && !["--write", "--check"].includes(process.argv[2]))
  throw new Error("Usage: generated.mjs [--write|--check]");
const pin = referencePin(root);
const head = execFileSync("git", ["-C", "ts-ref", "rev-parse", "HEAD"], {
  encoding: "utf8",
}).trim();
if (head !== pin.commit) throw new Error("Generator source does not match Cargo pin");
execFileSync("git", ["-C", "ts-ref", "diff", "--exit-code", "HEAD", "--", "tsc"], {
  stdio: "inherit",
});
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-port-generated-"));
const changed = [];
function generated(file, args) {
  const target = path.join(temporary, path.basename(file));
  fs.copyFileSync(file, target);
  execFileSync(process.execPath, [...args, target], { stdio: "inherit" });
  const config = ["rustfmt.toml", ".rustfmt.toml"].find((f) => fs.existsSync(f));
  execFileSync(
    "rustfmt",
    ["--edition", "2024", ...(config ? ["--config-path", path.resolve(config)] : []), target],
    { stdio: "inherit" },
  );
  const next = fs.readFileSync(target);
  if (!next.equals(fs.readFileSync(file))) {
    changed.push(file);
    if (write) fs.writeFileSync(file, next);
  }
}
try {
  const go = "ts-ref/tsc/internal/tsoptions/declarations_generated.go";
  generated("crates/tsrs_tsoptions/src/declscompiler.rs", [
    "tools/gen-tsoptions/gen_decls.mjs",
    "--vars",
    "commonOptionsWithBuild,optionsForCompiler",
    go,
  ]);
  generated("crates/tsrs_tsoptions/src/declsbuild.rs", [
    "tools/gen-tsoptions/gen_decls.mjs",
    "--vars",
    "TscBuildOption,OptionsForBuild",
    go,
  ]);
  generated("crates/tsrs_tsoptions/tests/integration/tsconfigparsing_test.rs", [
    "tools/gen-tsoptions/gen_tsconfig_tests.mjs",
    "ts-ref/tsc/internal/tsoptions/tsconfigparsing_test.go",
  ]);
  const table = "crates/tsrs_api/tests/integration/checker/coverage_table.rs";
  const current = fs.readFileSync(table, "utf8");
  const expected = [
    ...fs
      .readFileSync("ts-ref/tsc/internal/api/proto.go", "utf8")
      .matchAll(/^\s*Method\w+\s+Method\s*=\s*"([^"]+)"/gm),
  ].map((m) => m[1]);
  const actual = [...current.matchAll(/Row\s*\{\s*method:\s*"([^"]+)"/g)].map((m) => m[1]);
  if (
    !expected.length ||
    new Set(actual).size !== actual.length ||
    actual.length !== expected.length ||
    expected.some((name) => !actual.includes(name))
  ) {
    throw new Error(
      "API coverage inventory differs from proto.go; classify added methods and remove retired rows explicitly",
    );
  }
  const header = `// @generated from tsc/internal/api/proto.go at microsoft/TypeScript ${pin.commit.slice(0, 12)}. One row per Method constant.`;
  const next = current.replace(/^\/\/ @generated[^\n]*/, header);
  if (next !== current) {
    changed.push(table);
    if (write) fs.writeFileSync(table, next);
  }
  const goVersion = sourceVersion(root),
    rust = fs.readFileSync("crates/tsrs_core/src/version.rs", "utf8");
  if (!rust.includes(`None => "${goVersion}"`))
    throw new Error("Rust source-default version differs from the pinned Go source");
  if (changed.length && !write)
    throw new Error(
      `Generated regions are stale: ${changed.join(", ")}; run node tools/port/generated.mjs --write`,
    );
  console.log(`Extra generated regions and API inventory verified (${pin.commit.slice(0, 12)}).`);
} finally {
  fs.rmSync(temporary, { recursive: true, force: true });
}
