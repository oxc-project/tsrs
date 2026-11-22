#!/usr/bin/env node
// Apply upstream TypeScript fixes adopted before the pinned commit contains them.
import fs from "node:fs";
import path from "node:path";
import { ROOT, args, main, pinnedCommit, run, requireValue } from "../lib/cli.mjs";

main(() => {
    const options = args("Apply the backports for the pinned TypeScript commit to a ts-ref checkout.", {
        ref: "ts-ref",
        include: [],
    });
    const commit = pinnedCommit();
    const ref = path.resolve(ROOT, options.ref);
    const head = run(["git", "-C", ref, "rev-parse", "HEAD"]).stdout.trim();
    requireValue(head === commit, `${options.ref} is at ${head}, expected the TypeScript pin ${commit}`);
    const backports = path.join(ROOT, "tools/typescript-backports");
    for (const entry of fs.readdirSync(backports, { withFileTypes: true }))
        if (entry.isDirectory() && entry.name !== commit)
            console.warn(`::warning::tools/typescript-backports/${entry.name} targets another TypeScript pin`);
    const directory = path.join(backports, commit);
    const patches = fs.existsSync(directory)
        ? fs
              .readdirSync(directory)
              .filter((name) => name.endsWith(".patch"))
              .sort()
        : [];
    const filters = options.include.map((pattern) => `--include=${pattern}`);
    for (const name of patches) {
        const patch = path.join(directory, name);
        const apply = (...flags) =>
            run(["git", "-C", ref, "apply", "--whitespace=nowarn", ...filters, ...flags, patch], { check: false });
        if (apply("--check").code === 0) {
            const result = apply();
            requireValue(result.code === 0, `${name} failed to apply\n${result.stderr}`);
            console.log(`${name}: applied`);
        } else if (apply("--check", "--reverse").code === 0) console.log(`${name}: already applied`);
        else throw new Error(`${name} does not apply to ${options.ref}\n${apply("--check").stderr}`);
    }
    console.log(`${patches.length} TypeScript backport(s) for ${commit}`);
});
