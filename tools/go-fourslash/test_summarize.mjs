// Existing coverage-gate regression cases, ported from unittest.
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { run } from "../lib/cli.mjs";
const event = (Action, Test) => ({ Action, Package: "fourslash", ...(Test ? { Test } : {}) });
function summarize(events, minimum = 1) {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-summary-"));
    try {
        const file = path.join(dir, "results.jsonl");
        fs.writeFileSync(file, events.map((e) => JSON.stringify(e) + "\n").join(""));
        return run(
            [process.execPath, new URL("summarize.mjs", import.meta.url).pathname, file, "--min-pass", String(minimum)],
            { check: false },
        );
    } finally {
        fs.rmSync(dir, { recursive: true, force: true });
    }
}
test("pass floor", () => {
    const events = [event("pass", "TestA"), event("skip", "TestB"), event("pass")];
    assert.equal(summarize(events).code, 0);
    const r = summarize(events, 2);
    assert.equal(r.code, 1);
    assert.match(r.stdout, /minimum 2/);
});
test("skipped-only parents do not satisfy floor", () => {
    for (const child of ["TestA/child", "TestA/nested/child"]) {
        const events = [
                event("skip", child),
                ...(child.split("/").length === 3 ? [event("pass", "TestA/nested")] : []),
                event("pass", "TestA"),
                event("pass"),
            ],
            r = summarize(events);
        assert.equal(r.code, 1);
        assert.match(r.stdout, /pass=0/);
    }
});
test("mixed subtests count as one exercised group", () => {
    const e = [event("pass", "TestA/a"), event("skip", "TestA/b"), event("pass", "TestA"), event("pass")];
    assert.equal(summarize(e).code, 0);
    assert.equal(summarize(e, 2).code, 1);
});
test("failures cannot be offset by passes", () => {
    for (const failure of [[event("fail", "TestB")], [event("fail", "TestB/child"), event("fail", "TestB")]])
        assert.equal(summarize([event("pass", "TestA"), ...failure, event("fail")]).code, 1);
});
test("package failure and skip fail even with zero floor", () => {
    for (const a of ["fail", "skip"]) assert.equal(summarize([event(a)], 0).code, 1);
});
test("incomplete results fail", () => {
    const r = summarize([event("pass", "TestA")]);
    assert.equal(r.code, 2);
    assert.match(r.stderr, /incomplete results/);
});
