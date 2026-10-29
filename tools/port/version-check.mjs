#!/usr/bin/env node
// Exercise the published version stamp, incremental metadata and version-sensitive package resolution.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { publishedReference, referencePin, libraryHash } from "../lib/typescript-reference.mjs";
const candidate = path.resolve(process.argv[2] || "target/published/debug/tsrs");
const pin = referencePin(process.cwd()),
  reference = publishedReference(process.cwd());
const directory = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-version-"));
const write = (file, value) => {
  const p = path.join(directory, file);
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, typeof value === "string" ? value : JSON.stringify(value));
};
function invoke(binary, args) {
  const r = spawnSync(binary, args, { cwd: directory, encoding: "utf8" });
  if (r.error) throw r.error;
  if (r.signal || ![0, 1, 2].includes(r.status))
    throw new Error(`Compiler failed: ${r.signal || r.status}`);
  return { status: r.status, output: (r.stdout + r.stderr).replaceAll(directory, "<dir>") };
}
try {
  if (libraryHash(path.dirname(candidate)) !== reference.library_sha256)
    throw new Error("Candidate standard libraries differ from the published reference");
  const version = invoke(candidate, ["--version"]);
  if (
    version.status !== 0 ||
    !version.output.startsWith(`Version ${pin.nightly} (`) ||
    !version.output.includes(`microsoft/TypeScript@${pin.commit.slice(0, 12)}`)
  )
    throw new Error("Candidate version/source stamp differs from the published reference");
  write("tsconfig.json", {
    compilerOptions: {
      composite: true,
      incremental: true,
      noEmit: true,
      strict: true,
      module: "nodenext",
      target: "esnext",
      tsBuildInfoFile: "state.tsbuildinfo",
    },
    files: ["a.ts"],
  });
  write("a.ts", 'import { value } from "versioned";\nconst result: "nightly" = value;\n');
  write("node_modules/versioned/package.json", {
    name: "versioned",
    version: "1.0.0",
    types: "fallback.d.ts",
    typesVersions: {
      [">=" + pin.nightly]: { "*": ["nightly.d.ts"] },
      "*": { "*": ["fallback.d.ts"] },
    },
  });
  write("node_modules/versioned/nightly.d.ts", 'export declare const value: "nightly";\n');
  write("node_modules/versioned/fallback.d.ts", 'export declare const value: "fallback";\n');
  const results = [];
  for (const binary of [reference.binary, candidate]) {
    fs.rmSync(path.join(directory, "state.tsbuildinfo"), { force: true });
    const result = invoke(binary, ["-p", ".", "--pretty", "false", "--singleThreaded"]);
    const state = JSON.parse(fs.readFileSync(path.join(directory, "state.tsbuildinfo"), "utf8"));
    if (state.version !== pin.nightly)
      throw new Error("Incremental metadata has the wrong TypeScript version");
    const selected = state.fileNames
      .filter((name) => name.includes("/versioned/"))
      .map((name) => name.replaceAll("\\", "/"));
    if (selected.length !== 1)
      throw new Error("Version-sensitive package resolution was not exercised");
    results.push({ ...result, selected });
  }
  if (JSON.stringify(results[0]) !== JSON.stringify(results[1]))
    throw new Error(
      "Candidate differs from the published reference in typesVersions or diagnostic/exit behavior",
    );
  console.log("Published version, source stamp, tsbuildinfo and typesVersions match.");
} finally {
  fs.rmSync(directory, { recursive: true, force: true });
}
