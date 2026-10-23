#!/usr/bin/env node
import { args, read, requireValue, main, isMain } from "../../lib/cli.mjs";
const fields = {
    name: "string",
    identical: "integer",
    different: "integer",
    missing: "integer",
    extra: "integer",
    diagnostics_match: "boolean",
    rs_panic: "string",
};
export function rowErrors(row) {
    if (!row || typeof row !== "object" || Array.isArray(row)) return ["not a JSON object"];
    const errors = [];
    for (const [k, t] of Object.entries(fields))
        if (t === "integer" ? !Number.isInteger(row[k]) || row[k] < 0 : typeof row[k] !== t) errors.push(k);
    for (const k of ["ref_status", "rs_status"]) if (!Number.isInteger(row[k]) && row[k] !== "timeout") errors.push(k);
    return errors;
}
if (isMain(import.meta.url))
    main(() => {
        const a = args("Fail-closed emit report. <selected-names> <results.jsonl>");
        requireValue(a._.length === 2, "selection and results files are required");
        const selected = read(a._[0])
                .split("\n")
                .map((l) => l.trim())
                .filter(Boolean),
            rows = new Map(),
            problems = [],
            totals = { identical: 0, different: 0, missing: 0, extra: 0 };
        let failed = 0;
        if (!selected.length) problems.push("no package selected");
        if (new Set(selected).size !== selected.length) problems.push("duplicate names in the selection");
        read(a._[1])
            .split("\n")
            .forEach((line, n) => {
                if (!line.trim()) return;
                try {
                    const r = JSON.parse(line),
                        bad = rowErrors(r);
                    requireValue(!bad.length, `bad or missing fields ${bad}`);
                    if (rows.has(r.name)) problems.push(`duplicate result for ${r.name}`);
                    else if (!selected.includes(r.name)) problems.push(`unexpected result for ${r.name}`);
                    else rows.set(r.name, r);
                } catch (e) {
                    problems.push(`malformed result line ${n + 1}: ${e.message}: ${line.slice(0, 120)}`);
                }
            });
        for (const name of selected.sort()) {
            const r = rows.get(name);
            if (!r) {
                problems.push(`no result for ${name} (run.mjs died or produced no JSON)`);
                console.log(`${name.padEnd(45)} NO RESULT`);
                failed++;
                continue;
            }
            for (const k of Object.keys(totals)) totals[k] += r[k];
            const reasons = [];
            if (r.different || r.missing || r.extra) reasons.push("files");
            if (r.ref_status !== r.rs_status) reasons.push("exit status");
            if ([r.ref_status, r.rs_status].includes("timeout")) reasons.push("timeout");
            else if (![r.ref_status, r.rs_status].every((s) => [0, 1, 2, 3, 4].includes(s)))
                reasons.push("abnormal exit");
            if (!r.diagnostics_match) reasons.push("diagnostics");
            if (r.rs_panic) reasons.push("tsrs panic");
            if (reasons.length) failed++;
            console.log(
                `${name.padEnd(45)} ${(reasons.length ? `FAIL(${reasons.join(",")})` : "ok").padEnd(14)} identical ${String(r.identical).padStart(5)}  different ${String(r.different).padStart(4)}  not-emitted ${String(r.missing).padStart(5)}  extra ${String(r.extra).padStart(3)}  exit ${r.ref_status}/${r.rs_status}  diags ${r.diagnostics_match ? "=" : "!="}${r.rs_panic ? `  [${r.rs_panic.slice(0, 90)}]` : ""}`,
            );
        }
        console.log(
            `packages: ${selected.length} selected, ${selected.length - failed} identical, ${failed} failed; files: ${totals.identical} identical, ${totals.different} different, ${totals.missing} not emitted by tsrs, ${totals.extra} extra`,
        );
        for (const p of problems) console.log(`ERROR: ${p}`);
        const ok = !problems.length && !failed;
        console.log(`RESULT: ${ok ? "PASS" : "FAIL"}`);
        process.exitCode = ok ? 0 : 1;
    });
