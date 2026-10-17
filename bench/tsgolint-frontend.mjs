#!/usr/bin/env node
// Compare Oxlint helpers through the complete frontend on macOS.
import fs from "node:fs";
import path from "node:path";
import {
    appendSample,
    cleanEnv,
    counters,
    createOutput,
    diagnosticDigest,
    digest,
    parseCommandLine,
    parseHelpers,
    resolvePath,
    timed,
} from "./tsgolint.mjs";

async function main() {
    const { values: args, fail } = parseCommandLine(
        {
            frontend: { required: true },
            config: { required: true },
            cwd: { required: true },
            helper: { required: true, append: true },
            output: { required: true },
            runs: { type: "int", default: 5 },
            warmups: { type: "int", default: 1 },
        },
        "usage: tsgolint-frontend.mjs [-h] --frontend FRONTEND --config CONFIG --cwd CWD --helper NAME=PATH " +
            "--output OUTPUT [--runs RUNS] [--warmups WARMUPS]",
        "Compare Oxlint helpers through the complete frontend on macOS.",
    );
    if (args.runs < 1 || args.warmups < 0) fail("runs must be positive and warmups nonnegative");
    const helpers = parseHelpers(args.helper, fail);
    const frontend = fs.realpathSync(args.frontend);
    const config = fs.realpathSync(args.config);
    const output = createOutput(args.output);
    const metadata = {
        frontend: { path: frontend, sha256: digest(frontend) },
        config: { path: config, sha256: digest(config) },
        cwd: resolvePath(args.cwd),
        helpers: Object.fromEntries([...helpers].map(([name, file]) => [name, { path: file, sha256: digest(file) }])),
        runs: args.runs,
        warmups: args.warmups,
        mode: "default; thread overrides unset",
        rss_note: "Frontend time -l counters are not helper-only or simultaneous process-tree peaks.",
    };
    fs.writeFileSync(path.join(output, "metadata.json"), JSON.stringify(metadata, null, 2) + "\n");
    for (const variant of ["lint", "lint-typecheck"]) {
        const command = [frontend, "--type-aware", "--no-ignore", "-c", config, "--format", "json", "src"];
        if (variant === "lint-typecheck") command.push("--type-check");
        for (let repetition = -args.warmups; repetition < args.runs; repetition++) {
            const order = [...helpers.keys()];
            if (repetition >= 0 && repetition % 2) order.reverse();
            for (const name of order) {
                const label = `${variant}-${repetition}-${name}`;
                const env = cleanEnv();
                env.OXLINT_TSGOLINT_PATH = helpers.get(name);
                const stdout = fs.openSync(path.join(output, `${label}.json`), "w"),
                    stderr = fs.openSync(path.join(output, `${label}.err`), "w");
                let status, wall;
                try {
                    ({ code: status, wall } = await timed(
                        ["/usr/bin/time", "-l", "-o", path.join(output, `${label}.time`), ...command],
                        { cwd: args.cwd, env, stdio: ["ignore", stdout, stderr] },
                    ));
                } finally {
                    for (const fd of [stdout, stderr]) fs.closeSync(fd);
                }
                if (status !== 0 && status !== 1)
                    throw new Error(`${label} exited ${status}; inspect ${path.join(output, `${label}.err`)}`);
                const result = JSON.parse(fs.readFileSync(path.join(output, `${label}.json`), "utf8"));
                const summary = diagnosticDigest(result.diagnostics, (diagnostic) => diagnostic.code || "<frontend>");
                const sample = {
                    helper: name,
                    variant,
                    rep: repetition,
                    warmup: repetition < 0,
                    wall_s: wall,
                    exit_code: status,
                    ...counters(path.join(output, `${label}.time`)),
                    diagnostics: summary.count,
                    diagnostics_by_code: summary.counts,
                    diagnostic_multiset_sha256: summary.sha256,
                    reported_files: result.number_of_files ?? null,
                };
                appendSample(output, sample);
                console.log(`${label}: ${wall.toFixed(3)}s, ${sample.diagnostics} diagnostics`);
            }
        }
    }
}

main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
});
