#!/usr/bin/env node
// Compare ordered diagnostics, including their continuation lines and compiler exit codes.
import assert from "node:assert/strict";
import { args, read, required, requireValue, isMain, main } from "../lib/cli.mjs";
export const CRASH_RE = /^(?:panic: |fatal error: |thread '.*' panicked at )/m;
export function diagnostics(output) {
    const result = [];
    let inside = false;
    for (const line of output.split(/\r?\n/)) {
        if (/(^|: )error TS\d+:/.test(line)) {
            result.push(line);
            inside = true;
        } else if (inside && /^[ \t]/.test(line) && line.trim()) result.push(line);
        else inside = false;
    }
    return result;
}
export function compare(base, baseExit, current, currentExit, label, report = console.log) {
    if (![0, 1, 2].includes(currentExit) || CRASH_RE.test(current)) {
        report(`FAIL ${label}: new did not finish normally (exit ${currentExit})`);
        return false;
    }
    if (base === null || base === undefined) {
        report(`pass ${label}: ${diagnostics(current).length} ordered diagnostic lines, exit ${currentExit}`);
        return true;
    }
    if (![0, 1, 2].includes(baseExit) || CRASH_RE.test(base)) {
        report(
            `pass ${label}: base did not finish normally (exit ${baseExit}); new finished normally (exit ${currentExit}), skipping base diagnostic comparison`,
        );
        return true;
    }
    const problems = [];
    if (baseExit !== currentExit) problems.push(`exit differs: base ${baseExit}, new ${currentExit}`);
    const a = diagnostics(base),
        b = diagnostics(current);
    if (JSON.stringify(a) !== JSON.stringify(b)) {
        let i = 0;
        while (i < Math.min(a.length, b.length) && a[i] === b[i]) i++;
        problems.push(
            `diagnostic line ${i + 1} differs: base ${JSON.stringify(a[i] ?? "<end of diagnostics>")}, new ${JSON.stringify(b[i] ?? "<end of diagnostics>")}`,
        );
    }
    if (problems.length) {
        for (const problem of problems) report(`FAIL ${label}: ${problem}`);
        return false;
    }
    report(`pass ${label}: ${a.length} ordered diagnostic lines, exit ${baseExit}`);
    return true;
}
if (isMain(import.meta.url))
    main(() => {
        const a = args("Compare ordered TypeScript diagnostics and exit codes.", {
            "self-test": false,
            label: "run",
            base: null,
            "base-exit": null,
            new: null,
            "new-exit": null,
        });
        if (a["self-test"]) {
            const text = "a.ts(1,1): error TS1: first\n  detail\nb.ts(1,1): error TS2: second\n";
            assert(compare(text, 2, `noise\n${text}`, 2, "", () => {}));
            assert(!compare(text, 2, text, 0, "", () => {}));
            assert(!compare(text, 2, text.replace("detail", "changed"), 2, "", () => {}));
            console.log("self-test: ordered diagnostics and exit codes are checked");
            return;
        }
        required(a, "new", "new-exit");
        requireValue(
            (a.base === undefined) === (a["base-exit"] === undefined),
            "--base and --base-exit must be supplied together",
        );
        process.exitCode = compare(
            a.base === undefined ? null : read(a.base),
            a["base-exit"] === undefined ? null : Number(a["base-exit"]),
            read(a.new),
            Number(a["new-exit"]),
            a.label,
        )
            ? 0
            : 1;
    });
