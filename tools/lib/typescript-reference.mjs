#!/usr/bin/env node
// Resolve the published Go compiler from the exact Cargo/npm pin. Never run a candidate compiler as the oracle.
import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));
const hash = (file) => createHash("sha256").update(fs.readFileSync(file)).digest("hex");
export const libraryHash = (directory) => {
  const result = createHash("sha256");
  for (const file of fs
    .readdirSync(directory)
    .filter((name) => name.endsWith(".d.ts"))
    .sort())
    result
      .update(file)
      .update("\0")
      .update(fs.readFileSync(path.join(directory, file)));
  return result.digest("hex");
};

export function referencePin(root = ROOT) {
  const text = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8");
  const table = text.split("[workspace.metadata.typescript]")[1]?.split(/^\[/m)[0] || "";
  const get = (key) => new RegExp(`^${key}\\s*=\\s*"([^\\"]+)"`, "m").exec(table)?.[1];
  const pin = { version: get("version"), nightly: get("nightly"), commit: get("commit") };
  if (!pin.version || !pin.nightly || !/^[0-9a-f]{40}$/.test(pin.commit || ""))
    throw new Error("Invalid Cargo TypeScript reference");
  return pin;
}

export function sourceVersion(root = ROOT) {
  const go = fs.readFileSync(path.join(root, "ts-ref/tsc/internal/core/version.go"), "utf8");
  const version = /^var version = "([^"]+)"/m.exec(go)?.[1];
  if (!version) throw new Error("Cannot read the pinned Go source-default version");
  return version;
}

export function publishedReference(root = ROOT) {
  const pin = referencePin(root);
  const manifest = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
  if (manifest.devDependencies?.typescript !== pin.nightly)
    throw new Error("Root TypeScript dependency must match Cargo nightly exactly");
  const require = createRequire(path.join(root, "package.json"));
  let packageFile;
  try {
    packageFile = require.resolve("typescript/package.json");
  } catch {
    throw new Error("Published TypeScript reference is missing; run vp install --frozen-lockfile");
  }
  const pkg = JSON.parse(fs.readFileSync(packageFile, "utf8"));
  const native = `@typescript/typescript-${process.platform}-${process.arch}`;
  if (
    pkg.name !== "typescript" ||
    pkg.version !== pin.nightly ||
    pkg.gitHead !== pin.commit ||
    pkg.optionalDependencies?.[native] !== pin.nightly
  ) {
    throw new Error("Installed TypeScript package version/source does not match Cargo metadata");
  }
  const nativeFile = createRequire(packageFile).resolve(`${native}/package.json`);
  const installed = JSON.parse(fs.readFileSync(nativeFile, "utf8"));
  if (
    installed.name !== native ||
    installed.version !== pin.nightly ||
    installed.gitHead !== pin.commit
  ) {
    throw new Error("Native TypeScript package version/source does not match Cargo metadata");
  }
  const binary = path.join(
    path.dirname(nativeFile),
    "lib",
    process.platform === "win32" ? "tsc.exe" : "tsc",
  );
  const version = execFileSync(binary, ["--version"], { encoding: "utf8" }).trim();
  if (version !== `Version ${pin.nightly}`)
    throw new Error(`Unexpected published compiler version: ${version}`);
  return {
    binary,
    source_commit: pin.commit,
    library_sha256: libraryHash(path.dirname(binary)),
    provenance: {
      package: pkg.name,
      version: pkg.version,
      native_package: installed.name,
      native_version: installed.version,
      binary_sha256: hash(binary),
    },
  };
}

export function verifyReference(before, root = ROOT) {
  const after = publishedReference(root);
  if (JSON.stringify(after) !== JSON.stringify(before))
    throw new Error("Published reference changed during the port");
  return after;
}

export function installOracle(root = ROOT) {
  const reference = publishedReference(root);
  const directory = path.join(root, "tools/node-api/.work/published-oracle");
  fs.rmSync(directory, { recursive: true, force: true });
  // Keep standard libraries beside the copied executable, as in the native npm package.
  fs.cpSync(path.dirname(reference.binary), directory, { recursive: true, dereference: true });
  fs.writeFileSync(
    path.join(directory, "reference.json"),
    JSON.stringify(reference, null, 2) + "\n",
  );
  fs.writeFileSync(path.join(directory, "commit"), reference.source_commit + "\n");
  return checkOracle(root);
}

export function checkOracle(root = ROOT) {
  const current = publishedReference(root);
  const directory = path.join(root, "tools/node-api/.work/published-oracle");
  const recorded = JSON.parse(fs.readFileSync(path.join(directory, "reference.json"), "utf8"));
  const binary = path.join(directory, process.platform === "win32" ? "tsc.exe" : "tsc");
  if (
    JSON.stringify(recorded) !== JSON.stringify(current) ||
    hash(binary) !== current.provenance.binary_sha256 ||
    libraryHash(directory) !== current.library_sha256
  ) {
    throw new Error(
      "Node API oracle does not match the published TypeScript reference; rerun tools/node-api/setup.sh --published-reference",
    );
  }
  return { ...current, binary };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const mode = process.argv[2] || "--json";
    const reference =
      mode === "--install-oracle"
        ? installOracle()
        : mode === "--check-oracle"
          ? checkOracle()
          : publishedReference();
    if (mode === "--binary") console.log(reference.binary);
    else if (["--json", "--install-oracle", "--check-oracle"].includes(mode))
      console.log(JSON.stringify(reference));
    else
      throw new Error(
        "Usage: typescript-reference.mjs [--json|--binary|--install-oracle|--check-oracle]",
      );
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
