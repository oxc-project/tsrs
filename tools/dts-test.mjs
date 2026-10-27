#!/usr/bin/env node
import { mkdirSync } from "node:fs";
import path from "node:path";
import { ROOT, args, main, pinnedCommit, requireValue, run } from "./lib/cli.mjs";

main(() => {
    const options = args("Run declaration bundler tests. --install prepares frozen pnpm dependencies through vp install first.", {
        suite: "rust",
        profile: "dev",
        install: false,
    });
    requireValue(options._.length === 0, "Unexpected positional arguments; use --help.");
    requireValue(["rust", "vue", "all"].includes(options.suite), "--suite must be rust, vue or all");
    requireValue(["dev", "ci"].includes(options.profile), "--profile must be dev or ci");
    const rust = options.suite !== "vue";
    const vue = options.suite !== "rust";
    const execute = (command, extra = {}) => run(command, {
        cwd: ROOT, inherit: true, timeout: 1800, ...extra,
    });
    if (options.install) {
        for (const directory of [rust && "dependencies", vue && "vue"].filter(Boolean)) {
            execute(["vp", "install", "--frozen-lockfile", "--ignore-scripts", "--no-optional"], {
                cwd: path.join(ROOT, "crates/tsrs_dts/tests", directory),
            });
        }
    }
    if (rust) {
        execute(["cargo", "test", "--profile", options.profile, "--locked", "-p", "tsrs_dts"], {
            env: { ...process.env, TSRS_DTS_COMPARE_APIS: "1", TSRS_DTS_COMPARE_REUSE: "1" },
        });
    }
    if (vue) {
        const checkout = run(["git", "-C", "ts-ref", "rev-parse", "HEAD"], { cwd: ROOT });
        requireValue(checkout.stdout.trim() === pinnedCommit(), "ts-ref must match the workspace TypeScript pin");
        execute(["git", "-C", "ts-ref", "diff", "--exit-code", "HEAD", "--", "tsc"]);
        const reference = path.join(ROOT, "target/reference/tsgo");
        mkdirSync(path.dirname(reference), { recursive: true });
        // Rebuild from the verified checkout rather than trust an older reference binary.
        execute(["go", "-C", "ts-ref/tsc", "build", "-o", reference, "./cmd/tsc"], {
            env: { ...process.env, GOTOOLCHAIN: "auto", GOWORK: "off" },
        });
        execute(["cargo", "build", "--profile", options.profile, "--locked", "-p", "tsrs_cli"]);
        execute(["cargo", "build", "--profile", options.profile, "--locked", "-p", "tsrs_dts", "--example", "vue-test-host"]);
        const output = path.join("target", options.profile === "dev" ? "debug" : options.profile);
        const extension = process.platform === "win32" ? ".exe" : "";
        execute([process.execPath, "crates/tsrs_dts/tests/vue/run.mjs",
            path.join(output, `examples/vue-test-host${extension}`), path.join(output, `tsrs${extension}`), reference]);
    }
});
