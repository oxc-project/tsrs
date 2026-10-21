import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { restoreReleaseWasm } from "./restore-release-wasm.mjs";

const expected = { version: "0.18.0-ts7.1.0", typescriptCommit: "a".repeat(40) };
const wasm = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);

function fixture(t, { pkg = {}, bytes = wasm, manifest = {} } = {}) {
    const root = mkdtempSync(join(tmpdir(), "tsrs-release-wasm-"));
    t.after(() => rmSync(root, { recursive: true, force: true }));
    mkdirSync(join(root, "package"));
    writeFileSync(join(root, "package/package.json"), JSON.stringify({
        name: "@tsrs/wasm", version: expected.version,
        tsrs: { typescriptCommit: expected.typescriptCommit }, ...pkg,
    }));
    writeFileSync(join(root, "package/tsrs.wasm"), bytes);
    execFileSync("tar", ["-czf", join(root, "wasm.tgz"), "-C", root, "package"]);
    writeFileSync(join(root, "packages.json"), JSON.stringify({
        version: expected.version,
        packages: [{ name: "@tsrs/wasm", tarball: "/original/runner/wasm-dist/wasm.tgz" }],
        ...manifest,
    }));
    const destination = join(root, "checkout/tsrs.wasm");
    mkdirSync(join(root, "checkout"));
    writeFileSync(destination, "previous module");
    return { root, destination };
}

test("restores the release module byte-for-byte after moving the artifact", (t) => {
    const { root, destination } = fixture(t);
    restoreReleaseWasm(root, destination, expected);
    assert.deepEqual(readFileSync(destination), wasm);
});

for (const [name, overrides] of [
    ["wrong release manifest", { manifest: { version: "0.17.0" } }],
    ["wrong package", { pkg: { name: "tsrs" } }],
    ["wrong package version", { pkg: { version: "0.17.0" } }],
    ["wrong TypeScript commit", { pkg: { tsrs: { typescriptCommit: "b".repeat(40) } } }],
    ["invalid module", { bytes: Buffer.from("not wasm") }],
]) {
    test(`rejects ${name} without replacing the existing module`, (t) => {
        const { root, destination } = fixture(t, overrides);
        assert.throws(() => restoreReleaseWasm(root, destination, expected));
        assert.equal(readFileSync(destination, "utf8"), "previous module");
    });
}
