import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Read only the two package members needed here, without extracting archive paths into the checkout.
export function restoreReleaseWasm(artifact, destination, expected) {
    const manifest = JSON.parse(readFileSync(join(artifact, "packages.json"), "utf8"));
    if (manifest.version !== expected.version || manifest.packages?.length !== 1 ||
        manifest.packages[0].name !== "@tsrs/wasm") {
        throw new Error("Expected the WASM package for this release");
    }
    const filename = basename(manifest.packages[0].tarball);
    if (!filename.endsWith(".tgz")) throw new Error("Expected a packed WASM package");
    const archive = join(artifact, filename);
    const pkg = JSON.parse(execFileSync("tar", ["-xOf", archive, "package/package.json"], { encoding: "utf8" }));
    if (pkg.name !== "@tsrs/wasm" || pkg.version !== expected.version ||
        pkg.tsrs?.typescriptCommit !== expected.typescriptCommit) {
        throw new Error("WASM package metadata does not match this checkout");
    }
    const bytes = execFileSync("tar", ["-xOf", archive, "package/tsrs.wasm"], { maxBuffer: 64 * 1024 * 1024 });
    if (!WebAssembly.validate(bytes)) throw new Error("Invalid WebAssembly module in release package");
    mkdirSync(dirname(destination), { recursive: true });
    writeFileSync(destination, bytes);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    const root = fileURLToPath(new URL("../../", import.meta.url));
    const version = (flag) => execFileSync(process.execPath, [join(root, "npm/build.mjs"), flag], {
        cwd: root, encoding: "utf8",
    }).trim();
    if (process.argv.length !== 3) throw new Error("Usage: node restore-release-wasm.mjs <npm-wasm artifact directory>");
    restoreReleaseWasm(resolve(process.argv[2]), join(root, "npm/tsrs-wasm/tsrs.wasm"), {
        version: version("--print-version"),
        typescriptCommit: version("--print-typescript-commit"),
    });
}
