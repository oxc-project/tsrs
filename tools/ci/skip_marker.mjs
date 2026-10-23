#!/usr/bin/env node
import assert from "node:assert/strict";
import { args, read, run, required, main } from "../lib/cli.mjs";
const marker = /\[(skip ci|ci skip|no ci|skip actions|actions skip)\]/i;
export const isProse = (file) =>
    file.endsWith(".md") || ["notes/", "docs/", "bench/results/"].some((prefix) => file.startsWith(prefix));
export function violations(texts, files) {
    const marked = Object.entries(texts)
            .filter(([, text]) => marker.test(text ?? ""))
            .map(([name]) => name),
        code = files.filter((file) => !isProse(file));
    if (!marked.length || !code.length) return [];
    const shown = code.slice(0, 5).join(", ") + (code.length > 5 ? ` and ${code.length - 5} more` : "");
    return marked.map((name) => `${name} carries a CI-skip marker, but the pull request changes ${shown}`);
}
main(() => {
    const a = args("Reject CI-skip markers on changes to code.", {
        "title-file": null,
        "body-file": null,
        base: null,
        head: null,
        "self-test": false,
    });
    if (a["self-test"]) {
        assert.equal(violations({ title: "[skip ci]" }, ["notes/a.md"]).length, 0);
        assert.equal(violations({ body: "[SKIP CI]" }, ["bench/bench.mjs"]).length, 1);
        console.log("skip_marker self-test: ok");
        return;
    }
    required(a, "title-file", "body-file", "base", "head");
    const texts = { title: read(a["title-file"]), body: read(a["body-file"]) };
    run(["git", "log", "--format=%s", `${a.base}..${a.head}`])
        .stdout.trim()
        .split("\n")
        .forEach((s, i) => {
            texts[`commit ${i + 1} (${s.slice(0, 50)})`] = s;
        });
    const problems = violations(
        texts,
        run(["git", "diff", "--name-only", `${a.base}...${a.head}`])
            .stdout.trim()
            .split("\n"),
    );
    for (const p of problems)
        console.log(
            `::error::${p}: its squash commit would land on main without CI and without a bench run; rephrase (quote the marker in a code span with a space) or make the pull request notes-only`,
        );
    process.exitCode = problems.length ? 1 : 0;
});
