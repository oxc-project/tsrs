#!/usr/bin/env node
import { args, read, requireValue, main, isMain } from "../lib/cli.mjs";
export function summarize(events, minimum = 0) {
    requireValue(Number.isInteger(minimum) && minimum >= 0, "--min-pass must be nonnegative");
    const outcomes = new Map();
    let packageResult;
    for (const event of events)
        if (["pass", "fail", "skip"].includes(event.Action)) {
            if (event.Test !== undefined) outcomes.set(event.Test, event.Action);
            else packageResult = event.Action;
        }
    requireValue(packageResult !== undefined, "incomplete results: no final package outcome");
    const children = new Map();
    for (const name of outcomes.keys()) {
        const i = name.lastIndexOf("/");
        if (i >= 0) {
            const parent = name.slice(0, i);
            if (!children.has(parent)) children.set(parent, []);
            children.get(parent).push(name);
        }
    }
    const effective = new Map(outcomes);
    for (const name of [...outcomes.keys()].sort((a, b) => b.split("/").length - a.split("/").length))
        if (
            outcomes.get(name) === "pass" &&
            children.has(name) &&
            children.get(name).every((n) => effective.get(n) === "skip")
        )
            effective.set(name, "skip");
    const groups = [...outcomes.keys()].filter((n) => !n.includes("/")),
        count = (values) => values.reduce((o, v) => ({ ...o, [v]: (o[v] ?? 0) + 1 }), {});
    return {
        raw: count(groups.map((n) => outcomes.get(n))),
        counts: count(groups.map((n) => effective.get(n))),
        skippedParents: groups.filter((n) => outcomes.get(n) === "pass" && effective.get(n) === "skip").length,
        packageResult,
        failed:
            packageResult !== "pass" ||
            [...outcomes.values()].includes("fail") ||
            groups.filter((n) => effective.get(n) === "pass").length < minimum,
    };
}
if (isMain(import.meta.url))
    main(() => {
        const a = args("Summarize go test -json coverage. <results.jsonl>", { "min-pass": 0 });
        requireValue(a._.length === 1, "one results file is required");
        let result;
        try {
            result = summarize(read(a._[0]).trim().split("\n").filter(Boolean).map(JSON.parse), a["min-pass"]);
        } catch (error) {
            console.error(error.message);
            process.exitCode = 2;
            return;
        }
        console.log("Go top-level outcomes:", result.raw);
        console.log("Coverage (parents with only skipped subtests count as skipped):", result.counts);
        console.log("Parents with only skipped subtests:", result.skippedParents);
        if (result.failed)
            console.log(
                `fourslash gate failed: package=${result.packageResult}, pass=${result.counts.pass ?? 0} (minimum ${a["min-pass"]}), fail=${result.counts.fail ?? 0}`,
            );
        process.exitCode = result.failed ? 1 : 0;
    });
