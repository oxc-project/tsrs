import assert from "node:assert/strict";
import { test } from "node:test";
import { memoryFileSystem, runTsc } from "../core.js";

// A small module grows its memory and calls read, write and chtimes with paths above 128 MiB.
function highAddressHostCalls(path) {
    const leb = (value, signed = false) => {
        const out = [];
        for (;;) {
            const byte = value & 127;
            value >>= 7;
            const done = signed ? value === 0 && !(byte & 64) || value === -1 && (byte & 64) : value === 0;
            out.push(byte | (done ? 0 : 128));
            if (done) return out;
        }
    };
    const str = (s) => { const bytes = new TextEncoder().encode(s); return [...leb(bytes.length), ...bytes]; };
    const section = (id, items) => {
        const body = [...leb(items.length), ...items.flat()];
        return [id, ...leb(body.length), ...body];
    };
    const constant = (n) => [0x41, ...leb(n, true)];
    const body = (instructions) => { const bytes = [0, ...instructions, 0x0b]; return [...leb(bytes.length), ...bytes]; };
    const write = new TextEncoder().encode(`${path}\0hello`);
    const times = new TextEncoder().encode([path, "11", "22"].join("\0"));
    const data = [...write, ...times];
    const address = (128 << 20) + 64;
    const call = (op, ptr, len) => [...constant(op), ...constant(ptr), ...constant(len), 0x10, 0];
    const exports = [["memory", 2, 0], ["tsrs_input", 0, 1], ["tsrs_run", 0, 2], ["tsrs_output", 0, 3], ["tsrs_output_len", 0, 3]];
    return new WebAssembly.Module(new Uint8Array([
        0, 0x61, 0x73, 0x6d, 1, 0, 0, 0,
        ...section(1, [[0x60, 1, 0x7f, 1, 0x7f], [0x60, 0, 1, 0x7f], [0x60, 3, 0x7f, 0x7f, 0x7f, 1, 0x7f]]),
        ...section(2, [[...str("tsrs_host"), ...str("fs"), 0, 2]]),
        ...section(3, [[0], [1], [1]]),
        ...section(5, [[0, 1]]),
        ...section(7, exports.map(([name, kind, index]) => [...str(name), kind, index])),
        ...section(10, [body(constant(0)), body([
            ...constant(2049), 0x40, 0, 0x1a, // memory.grow; drop
            ...constant(address), ...constant(1024), ...constant(data.length), 0xfc, 0x0a, 0, 0, // memory.copy
            ...call(0, address, new TextEncoder().encode(path).length),
            ...call(4, address, write.length), 0x72, // i32.or
            ...call(7, address + write.length, times.length), 0x72,
        ]), body(constant(0))]),
        ...section(11, [[0, ...constant(1024), 0x0b, ...leb(data.length), ...data]]),
    ]));
}

test("host paths above 128 MiB survive the Workers subarray constructor limit", () => {
    const path = "/p/é😀.ts";
    const module = highAddressHostCalls(path);
    const host = memoryFileSystem({ [path]: "" });
    const times = [];
    host.chtimes = (...args) => times.push(args);
    const NativeArray = Uint8Array;
    // The hosted runtime rejects subarray's species construction at these offsets,
    // while direct views succeed. Keep the limit isolated to that constructor path.
    class LimitedSpecies extends NativeArray {
        constructor(buffer, offset, length) {
            if (buffer instanceof ArrayBuffer && offset > (128 << 20)) throw new RangeError("Invalid array buffer length");
            super(buffer, offset, length);
        }
    }
    class RuntimeArray extends NativeArray {
        static get [Symbol.species]() { return LimitedSpecies; }
    }
    let result;
    try {
        globalThis.Uint8Array = RuntimeArray;
        result = runTsc(module, { cwd: "/p", args: [], flags: 0 }, host);
    } finally {
        globalThis.Uint8Array = NativeArray;
    }
    assert.equal(result.exitCode, 0);
    assert.equal(result.memoryBytes, 2050 * 65536);
    assert.equal(host.written.get(path), "hello");
    assert.deepEqual(times, [[path, 11n, 22n]]);
});
