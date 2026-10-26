// Regression for #806: fs_take must treat wasm32 pointers as unsigned offsets.
// A tiny module reserves 2 GiB + 64 KiB of linear memory and asks the host to
// copy a staged file into it at exactly address 0x80000000. Before the fix the
// host passed the signed i32 to Uint8Array#set and threw RangeError.

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { memoryFileSystem, runTsc } from "../core.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const wasmPath = path.join(__dirname, "fixtures", "fs-take-high-address.wasm");

test("fs_take accepts a wasm32 pointer at 2 GiB", async () => {
    const module = new WebAssembly.Module(fs.readFileSync(wasmPath));

    let captured;
    const result = runTsc(
        module,
        { cwd: "/", args: [], flags: 0 },
        memoryFileSystem({ "/input": "hello" }),
        {
            afterRun(memory) {
                // fs_take wrote the staged "hello" bytes at 0x80000000.
                captured = new Uint8Array(memory.buffer, 0x80000000, 5).slice();
            },
        },
    );

    assert.equal(result.exitCode, 0);
    assert.equal(new TextDecoder().decode(captured), "hello");
});
