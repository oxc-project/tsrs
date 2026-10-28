// Builds the addon with Cargo and copies it to src-js/tsrs.node, which index.mjs loads. Pass --release for the
// release profile. A plain `cargo build -p tsrs_napi` shares the workspace's artifacts (no `--target` directory).
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const release = process.argv.includes("--release");
const root = fileURLToPath(new URL("../../", import.meta.url));
execFileSync("cargo", ["build", "--locked", "-p", "tsrs_napi", ...(release ? ["--release"] : [])], { cwd: root, stdio: "inherit" });
const library = { darwin: "libtsrs_napi.dylib", win32: "tsrs_napi.dll" }[process.platform] ?? "libtsrs_napi.so";
const built = path.resolve(root, process.env.CARGO_TARGET_DIR ?? "target", release ? "release" : "debug", library);
fs.copyFileSync(built, fileURLToPath(new URL("../src-js/tsrs.node", import.meta.url)));
