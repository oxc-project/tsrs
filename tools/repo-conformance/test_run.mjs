// Existing comparison, isolation, recovery and compiler controls, ported from unittest.
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {
    HARNESS,
    MODES,
    checkConfig,
    checkProjects,
    childEnv,
    compare,
    difference,
    execute,
    installPlan,
    summarize,
    checkDispatch,
    checkRerun,
} from "./run.mjs";
import { write, writeJSON, read, run, sleep } from "../lib/cli.mjs";
const temporary = async (fn) => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-conformance-"));
    try {
        return await fn(root);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
};
const context = (tsgo, tsrs, root) => ({
    tsgo,
    tsrs,
    env: childEnv(path.join(root, "home"), path.join(root, "cache")),
});
const compilation = (diagnostics = "", files = 1, exit = 0, timeout = false) => ({ diagnostics, files, exit, timeout });
const executable = (file, source) => {
    write(file, "#!/usr/bin/env node\n" + source);
    fs.chmodSync(file, 0o755);
    return file;
};
const compilers = (root) => {
    for (const n of ["tsgo", "tsrs"])
        executable(
            path.join(root, n),
            `const rust=require('node:path').basename(process.argv[1])==='tsrs';const strict=process.argv.includes('--noLazyMembers');const properties=rust&&!strict?'status: string; user?: undefined;':'user?: undefined; status: string;';console.log(\`b.ts(2,3): error TS2339: Property 'missing' does not exist on type '{ \${properties} }'.\`);console.log('Files: 2');process.exitCode=2;`,
        );
    return context(path.join(root, "tsgo"), path.join(root, "tsrs"), root);
};
test("dropped, changed or reordered diagnostics fail parity", () => {
    const first = "a.ts(1,1): error TS2322: wrong type.\n  'foo' is missing.",
        second = "b.ts(1,1): error TS2345: wrong argument.",
        base = compilation(first + "\n" + second, 1, 2);
    for (const text of [second, first.replace("foo", "bar") + "\n" + second, second + "\n" + first])
        assert.equal(compare(base, compilation(text, 1, 2)), "mismatch");
});
test("exit codes and loaded file counts must match", () => {
    assert.equal(compare(compilation(), compilation("", 1, 2)), "mismatch");
    assert.equal(compare(compilation(), compilation("", 2)), "mismatch");
});
test("crashes, timeouts and empty projects are not matches", () => {
    for (const [code, timeout] of [
        [-9, false],
        [101, false],
        [0, true],
    ]) {
        const broken = compilation("", 1, code, timeout);
        assert.equal(compare(compilation(), broken), "tsrs_failed");
        assert.equal(compare(broken, broken), "reference_failed");
    }
    assert.equal(compare(compilation("", 0), compilation("", 0)), "no_inputs");
});
test("differences separate diagnostics from other output", () => {
    const error = "a.ts(1,1): error TS2345: wrong argument.\n  Detail.",
        go = compilation("======== Resolving 'x' ========\nFound 'package.json'.\n" + error, 1, 2),
        traces = compilation(
            "======== Resolving 'x' ========\nExists according to earlier cached lookups.\n" + error,
            1,
            2,
        );
    assert.equal(compare(go, traces), "mismatch");
    assert.deepEqual(difference(go, traces), ["other output"]);
    assert.deepEqual(difference(go, compilation(go.diagnostics.replace("Detail.", "Other detail."), 1, 2)), [
        "diagnostics",
    ]);
    assert.deepEqual(difference(go, compilation(go.diagnostics, 2, 1)), ["exit code", "loaded files"]);
});
test("parsing keeps multiline diagnostics and excludes counters", () =>
    temporary(async (root) => {
        const bin = executable(
                path.join(root, "compiler"),
                `console.log(process.cwd()+'/a.ts(1,1): error TS2322: wrong.\\r\\n  Detail.\\r\\nFiles: 3\\r\\nCheck time: 0.01s\\r\\nFS: stat: 5');process.exitCode=2;`,
            ),
            ctx = context(bin, bin, root);
        const result = await checkConfig(root, "tsconfig.json", ctx, path.join(root, "logs"), 10);
        assert.equal(result.status, "match");
        assert.equal(result.tsgo.files, 3);
        assert.match(read(path.join(root, "logs/tsrs.log")), /Detail/);
    }));
test("strict mode does not clear a default difference", () =>
    temporary(async (root) => {
        const ctx = compilers(root),
            result = await checkProjects(root, ["tsconfig.json"], ctx, path.join(root, "logs"), 60, 5),
            c = result.configs[0];
        assert.equal(c.status, "mismatch");
        assert.deepEqual(c.difference, ["diagnostics"]);
        assert.equal(c.strict.status, "match");
        assert.equal(result.status, "mismatch");
        assert.equal(result.strict_status, "match");
        assert(fs.existsSync(path.join(root, "logs/0/diagnostics.diff")));
        assert(!fs.existsSync(path.join(root, "logs/0/diagnostics-strict.diff")));
        const manifest = {
            tsrs_commit: "rust-pin",
            typescript_commit: "go-pin",
            shards: 1,
            repositories: [{ repository: "a/one" }],
        };
        writeJSON(path.join(root, "results/shard-0.json"), {
            tsrs_commit: "rust-pin",
            typescript_commit: "go-pin",
            shard: 0,
            modes: MODES,
            results: [{ ...result, repository: "a/one", available_configs: 1 }],
        });
        assert.equal(summarize(manifest, path.join(root, "results"), path.join(root, "report.md")), 3);
        const text = read(path.join(root, "report.md"));
        assert.match(text, /1 different/);
        assert.match(text, /1 identical, 0 different, 0 tsrs failures in strict mode/);
        assert.match(text, /tsconfig.json: mismatch \(diagnostics\); strict match/);
    }));
test("timeout kills child processes", () =>
    temporary(async (root) => {
        const marker = path.join(root, "escaped-child"),
            script = `require('node:child_process').spawn(process.execPath,['-e',${JSON.stringify(`setTimeout(()=>require('node:fs').writeFileSync(${JSON.stringify(marker)},''),500);`)}],{stdio:'ignore'});setTimeout(()=>{},30000);`;
        const result = await execute(
            [process.execPath, "-e", script],
            root,
            path.join(root, "timeout.log"),
            0.2,
            childEnv(path.join(root, "home"), path.join(root, "cache")),
        );
        assert(result.timeout);
        await sleep(600);
        assert(!fs.existsSync(marker));
    }));
test("children get fresh configuration without host credentials", () =>
    temporary((root) => {
        const host = path.join(root, "host-home");
        write(path.join(host, ".npmrc"), "init-author-name=sentinel-host-npmrc\n");
        write(path.join(host, ".gitconfig"), "[user]\n name = sentinel-host-gitconfig\n");
        const ambient = {
                HOME: host,
                XDG_CONFIG_HOME: host,
                XDG_CACHE_HOME: path.join(root, "ambient-cache"),
                NPM_TOKEN: "sentinel-npm-token",
                GH_TOKEN: "sentinel-gh-token",
                npm_config_userconfig: path.join(host, ".npmrc"),
            },
            saved = Object.fromEntries(Object.keys(ambient).map((k) => [k, process.env[k]]));
        let env;
        try {
            Object.assign(process.env, ambient);
            env = childEnv(path.join(root, "home"), path.join(root, "cache"));
        } finally {
            for (const [k, v] of Object.entries(saved))
                if (v === undefined) delete process.env[k];
                else process.env[k] = v;
        }
        const cmd = (...argv) => run(argv, { cwd: root, env, check: false }).stdout.trim();
        assert.equal(
            cmd(
                process.execPath,
                "-e",
                "console.log(JSON.stringify(Object.values(process.env).filter(v=>v.includes('sentinel'))))",
            ),
            "[]",
        );
        assert.equal(cmd("npm", "config", "get", "init-author-name"), "");
        assert.equal(cmd("npm", "config", "get", "userconfig"), path.join(root, "home/.npmrc"));
        assert.equal(cmd("npm", "config", "get", "cache"), path.join(root, "cache/npm"));
        assert.equal(cmd("git", "config", "user.name"), "");
        assert(!fs.existsSync(path.join(root, "ambient-cache")));
    }));
async function plan(files) {
    return temporary((root) => {
        for (const [name, text] of Object.entries(files))
            write(path.join(root, name), typeof text === "string" ? text : JSON.stringify(text));
        return installPlan(root);
    });
}
test("frozen installs use the manager that wrote the lockfile", async () => {
    const pnpm9 = "lockfileVersion: '9.0'\n",
        cases = [
            [{ "package.json": {}, "pnpm-lock.yaml": pnpm9 }, "corepack pnpm@12.10.1 install --frozen-lockfile"],
            [{ "package.json": { pnpm: { overrides: {} } }, "pnpm-lock.yaml": pnpm9 }, "corepack pnpm@10.33.0 install"],
            [
                { "package.json": { workspaces: ["packages/*"] }, "pnpm-lock.yaml": pnpm9 },
                "corepack pnpm@10.33.0 install",
            ],
            [{ "package.json": {}, "pnpm-lock.yaml": "lockfileVersion: '6.0'\n" }, "corepack pnpm@8.15.9 install"],
            [{ "package.json": { packageManager: "pnpm@10.23.0" }, "bun.lock": "{}" }, "bun install --frozen-lockfile"],
            [
                { "package.json": { packageManager: "nub@0.8.3" }, "pnpm-lock.yaml": pnpm9 },
                "nub install --frozen-lockfile",
            ],
            [
                {
                    "package.json": { devEngines: { packageManager: { name: "pnpm", version: "11.11.0" } } },
                    "pnpm-lock.yaml": pnpm9,
                },
                "corepack pnpm@11.11.0 install --frozen-lockfile",
            ],
            [
                { "package.json": { packageManager: "pnpm@12.4.1" } },
                "corepack pnpm@12.4.1 install --no-frozen-lockfile",
            ],
        ];
    for (const [files, prefix] of cases) {
        const p = await plan(files);
        assert(p.command.join(" ").startsWith(prefix));
        assert(p.command.includes("--ignore-scripts"));
    }
    assert.deepEqual((await plan(cases[4][0])).tool, ["bun", "1.4.3"]);
    assert.equal((await plan({ "package.json": {} })).dependencies, "unpinned");
    await assert.rejects(plan({ "package.json": { packageManager: "utoo@1.0.0" } }), /Unsupported package manager/);
});
test("manager versions cannot select another executable", async () => {
    for (const name of ["npm", "pnpm", "yarn", "bun", "nub"])
        for (const version of [
            "npm:evil@1.0.0",
            "https://example.org/tool.tgz",
            "file:/tmp/tool",
            "latest",
            "^1.2.3",
            "1.2.3 --prefix=/tmp",
            "1.2.3+sha224.evil",
            "01.2.3",
            "1.2.3-01",
        ])
            await assert.rejects(plan({ "package.json": { packageManager: `${name}@${version}` } }), /Invalid/);
    for (const name of ["npm", "pnpm", "yarn"]) {
        const spec = `${name}@1.2.3+sha224.` + "a".repeat(56);
        assert.deepEqual((await plan({ "package.json": { packageManager: spec } })).command.slice(0, 2), [
            "corepack",
            spec,
        ]);
    }
    for (const name of ["bun", "nub"])
        assert.deepEqual((await plan({ "package.json": { packageManager: `${name}@1.2.3-beta.1` } })).tool, [
            name,
            "1.2.3-beta.1",
        ]);
});
test("pnpm hooks are disabled", async () =>
    assert((await plan({ "package.json": { packageManager: "pnpm@10.23.0" } })).command.includes("--ignore-pnpmfile")));
test("rerun inputs are a valid pair", () => {
    for (const [id, shards, ok] of [
        ["", "", true],
        ["38278199589", "1, 4", true],
        ["", "1", false],
        ["38278199589", "", false],
        ["latest", "1", false],
        ["38278199589", "20", false],
        ["38278199589", "1;2", false],
    ])
        if (ok) assert.doesNotThrow(() => checkDispatch(id, shards));
        else assert.throws(() => checkDispatch(id, shards));
});
test("reruns require a pinned manifest from the named run", () =>
    temporary((root) => {
        const sha = "0123456789abcdef0123456789abcdef01234567",
            manifest = {
                harness: HARNESS,
                tsrs_commit: sha,
                typescript_commit: sha,
                shards: 3,
                repositories: [{ repository: "a/one", commit: sha }],
            },
            earlier = {
                id: 7,
                path: ".github/workflows/repo-conformance.yml",
                event: "workflow_dispatch",
                head_sha: sha,
                repository: { full_name: "o/tsrs" },
                head_repository: { full_name: "o/tsrs" },
            };
        for (const n of ["tsgo", "tsrs"]) write(path.join(root, "bin", n), "");
        const cases = [
            [{}, {}, "2,0", true],
            [{ harness: null }, {}, "1", false],
            [{ harness: HARNESS - 1 }, {}, "1", false],
            [{}, { event: "pull_request", head_sha: "f".repeat(40) }, "0,2", true],
            [{}, { event: "pull_request", head_repository: { full_name: "fork/tsrs" } }, "1", false],
            [{}, { head_repository: null }, "1", false],
            [{ repositories: [{ repository: "a/one" }] }, {}, "1", false],
            [{}, { head_sha: "f".repeat(40) }, "1", false],
            [{}, { path: ".github/workflows/ci.yml" }, "1", false],
            [{}, {}, "3", false],
        ];
        for (const [m, r, shards, ok] of cases)
            if (ok) assert.deepEqual(checkRerun({ ...manifest, ...m }, shards, { ...earlier, ...r }, root), [0, 2]);
            else assert.throws(() => checkRerun({ ...manifest, ...m }, shards, { ...earlier, ...r }, root));
        assert.throws(() => checkRerun({ ...manifest, recovery_of_run_id: 7 }, "0,2", earlier, root), /original run 7/);
        fs.rmSync(path.join(root, "bin/tsrs"));
        assert.throws(() => checkRerun(manifest, "1", earlier, root));
    }));
const manifest = (repos, shards = 1) => ({
    tsrs_commit: "rust-pin",
    typescript_commit: "go-pin",
    shards,
    repositories: repos.map((r) => (typeof r === "string" ? { repository: r } : r)),
});
const repoResult = (name, status = "match", commit, strict) => ({
    repository: name,
    ...(commit ? { commit } : {}),
    status,
    strict_status: strict,
    available_configs: 1,
    configs: [
        { config: "tsconfig.json", status, tsgo: { exit: 0 }, ...(strict ? { strict: { status: strict } } : {}) },
    ],
});
const shard = (index, results, settings = {}) => ({
    tsrs_commit: "rust-pin",
    typescript_commit: "go-pin",
    shard: index,
    modes: MODES,
    results,
    ...settings,
});
test("report exits distinguish parity and incomplete results", () =>
    temporary((root) => {
        const m = manifest(["a/one", "a/two"]),
            dir = path.join(root, "results"),
            out = path.join(root, "report.md"),
            report = (entries) => {
                writeJSON(path.join(dir, "shard-0.json"), shard(0, entries));
                return summarize(m, dir, out);
            };
        assert.equal(report([repoResult("a/one"), repoResult("a/two")]), 0);
        for (const status of ["mismatch", "tsrs_failed"]) {
            assert.equal(report([repoResult("a/one"), repoResult("a/two", status)]), 3);
            assert(read(out).includes(status));
        }
        assert.equal(report([repoResult("a/one")]), 2);
        assert.match(read(out), /Missing repository result: a\/two/);
    }));
test("missing, duplicate and wrong-revision shards fail", () =>
    temporary((root) => {
        const m = manifest(["a/one", "a/two"], 2),
            dir = path.join(root, "results"),
            out = path.join(root, "report.md");
        writeJSON(path.join(dir, "shard-0.json"), shard(0, [repoResult("a/one")]));
        assert.equal(summarize(m, dir, out), 2);
        writeJSON(path.join(dir, "shard-1.json"), shard(1, [repoResult("a/two")]));
        assert.equal(summarize(m, dir, out), 0);
        writeJSON(path.join(dir, "duplicate/shard-0.json"), shard(0, [repoResult("a/one")]));
        assert.equal(summarize(m, dir, out), 2);
        fs.rmSync(path.join(dir, "duplicate"), { recursive: true });
        for (const key of ["tsrs_commit", "typescript_commit"]) {
            writeJSON(path.join(dir, "shard-1.json"), shard(1, [repoResult("a/two")], { [key]: "other-compiler" }));
            assert.equal(summarize(m, dir, out), 2);
            assert.match(read(out), /Missing repository result: a\/two/);
        }
    }));
test("lost shards are recovered only with matching provenance", () =>
    temporary((root) => {
        const m = manifest(
                [
                    { repository: "a/one", commit: "c1" },
                    { repository: "a/two", commit: "c2" },
                    { repository: "a/three", commit: "c3" },
                ],
                2,
            ),
            prev = path.join(root, "previous"),
            next = path.join(root, "rerun"),
            out = path.join(root, "report.md"),
            base = shard(0, [repoResult("a/one", "match", "c1"), repoResult("a/three", "match", "c3")], {
                check_timeout: 120,
            });
        writeJSON(path.join(prev, "shard-0.json"), base);
        const report = () => summarize(m, next, out, prev);
        writeJSON(
            path.join(next, "shard-1.json"),
            shard(1, [repoResult("a/two", "match", "c2")], { check_timeout: 120 }),
        );
        assert.equal(report(), 0);
        for (const [commit, status, configs, accepted] of [
            ["newer", "match", true, false],
            [null, "match", true, false],
            [null, "setup_failed", false, true],
        ]) {
            const r = repoResult("a/two", status, commit);
            if (!configs) r.configs = [];
            writeJSON(path.join(next, "shard-1.json"), shard(1, [r], { check_timeout: 120 }));
            assert.equal(report() === 0, accepted);
        }
        writeJSON(
            path.join(next, "shard-1.json"),
            shard(1, [repoResult("a/one", "match", "c1")], { check_timeout: 120 }),
        );
        assert.equal(report(), 2);
        assert.match(read(out), /outside shard 1/);
        writeJSON(
            path.join(next, "shard-1.json"),
            shard(1, [repoResult("a/two", "match", "c2")], { check_timeout: 120 }),
        );
        for (const settings of [
            { check_timeout: 60 },
            { modes: { default: MODES.default } },
            { modes: ["default", "go"] },
        ]) {
            writeJSON(path.join(prev, "shard-0.json"), { ...base, ...settings });
            assert.equal(report(), 2);
        }
    }));
test("reruns cannot clear differences or replace complete shards", async () => {
    const m = manifest(
            ["1", "2", "3", "4"].map((n) => ({ repository: "a/" + n, commit: "c" + n })),
            2,
        ),
        r = (n, status = "match", strict = "match") => repoResult("a/" + n, status, "c" + n, strict),
        cases = [
            [[shard(1, [r("2", "mismatch")])], true],
            [[shard(1, [r("2", "match", "tsrs_failed")])], true],
            [[shard(1, [r("2", "tsrs_failed")], { modes: { default: MODES.default } })], true],
            [[shard(1, [r("2", "mismatch"), r("4")])], true],
            [[shard(1, [r("2"), r("4")])], true],
            [[shard(1, [r("2")]), shard(1, [r("2", "mismatch")])], true],
            [[shard(1, [r("2")])], false],
        ];
    for (const [earlier, error] of cases)
        await temporary((root) => {
            const prev = path.join(root, "previous"),
                next = path.join(root, "rerun"),
                out = path.join(root, "report.md");
            writeJSON(path.join(prev, "copy/shard-0.json"), shard(0, [r("1"), r("3")]));
            earlier.forEach((s, i) => writeJSON(path.join(prev, `copy-${i}/shard-1.json`), s));
            writeJSON(path.join(next, "shard-1.json"), shard(1, [r("2"), r("4")]));
            assert.equal(summarize(m, next, out, prev), error ? 2 : 0);
            assert.match(read(out), /4 of 4 repositories/);
        });
});
test("empty projects fail a run without comparisons", () =>
    temporary((root) => {
        writeJSON(
            path.join(root, "shard-0.json"),
            shard(0, [
                {
                    repository: "a/one",
                    status: "not_checked",
                    configs: [{ config: "tsconfig.json", status: "no_inputs" }],
                },
            ]),
        );
        assert.equal(summarize(manifest(["a/one"]), root, path.join(root, "report.md")), 2);
        assert.match(read(path.join(root, "report.md")), /No project completed/);
    }));
const realOptions = { skip: !process.env.TSRS_CONFORMANCE_TSRS || !process.env.TSRS_CONFORMANCE_TSGO };
const realContext = (root) =>
    context(path.resolve(process.env.TSRS_CONFORMANCE_TSGO), path.resolve(process.env.TSRS_CONFORMANCE_TSRS), root);
test("repeated composite checks do not reuse diagnostics", realOptions, () =>
    temporary(async (root) => {
        writeJSON(path.join(root, "tsconfig.json"), {
            compilerOptions: { strict: true, composite: true },
            files: ["a.ts"],
        });
        for (const [source, code] of [
            ["const x: number = 1;\n", 0],
            ["const x: number = 'no';\n", 2],
            ["const x: number = 1;\n", 0],
        ]) {
            write(path.join(root, "a.ts"), source);
            const r = await checkConfig(root, "tsconfig.json", realContext(root), path.join(root, "logs"), 30);
            assert.equal(r.status, "match");
            assert.equal(r.tsgo.exit, code);
            assert(r.tsgo.files > 0);
            if (code) assert.match(read(path.join(root, "logs/tsrs.log")), /error TS2322/);
            assert(!fs.existsSync(path.join(root, "a.js")));
            assert(!fs.readdirSync(path.join(root, "logs")).some((f) => f.endsWith(".tsbuildinfo")));
        }
    }),
);
test("solution references are compared and only loaded projects count", realOptions, () =>
    temporary((root) =>
        temporary(async (host) => {
            writeJSON(path.join(host, "tsconfig.json"), { files: ["a.ts"] });
            write(path.join(host, "a.ts"), "export {};\n");
            writeJSON(path.join(root, "tsconfig.json"), {
                files: [],
                references: [{ path: "./tsconfig.app.json" }, { path: "lib" }, { path: host }],
            });
            writeJSON(path.join(root, "tsconfig.app.json"), { compilerOptions: { strict: true }, files: ["app.ts"] });
            writeJSON(path.join(root, "lib/tsconfig.json"), {
                compilerOptions: { composite: true },
                files: ["lib.ts"],
            });
            writeJSON(path.join(root, "empty/tsconfig.json"), { files: [] });
            writeJSON(path.join(root, "solution/tsconfig.json"), { files: [], references: [{ path: "../lib" }] });
            write(path.join(root, "app.ts"), "const x: number = 'no';\n");
            write(path.join(root, "lib/lib.ts"), "export const y = 1;\n");
            const configs = ["tsconfig.json", "empty/tsconfig.json", "lib/tsconfig.json", "solution/tsconfig.json"],
                ctx = realContext(root);
            let r = await checkProjects(root, configs, ctx, path.join(root, "logs"), 60, 2);
            assert.deepEqual(
                r.configs.map((c) => [c.config, c.status]),
                [
                    ["tsconfig.json", "references"],
                    ["tsconfig.app.json", "match"],
                    ["lib/tsconfig.json", "match"],
                ],
            );
            assert.equal(r.status, "partial");
            assert.equal(r.configs[1].tsgo.exit, 2);
            r = await checkProjects(root, configs, ctx, path.join(root, "logs"), 60, 5);
            assert.deepEqual(
                r.configs.slice(3).map((c) => c.status),
                ["no_inputs", "no_inputs"],
            );
            assert.equal(r.status, "partial");
        }),
    ),
);
test("widened optional properties match in both modes", realOptions, () =>
    temporary(async (root) => {
        writeJSON(path.join(root, "tsconfig.json"), { compilerOptions: { strict: true }, files: ["a.ts", "b.ts"] });
        write(path.join(root, "a.ts"), 'export const a = Math.random() > 0.5 ? { user: 1 } : { status: "a" };\n');
        write(
            path.join(root, "b.ts"),
            'export const b = Math.random() > 0.5 ? { status: "no" } : { status: "ok", user: 2 };\nb.missing;\n',
        );
        const r = await checkProjects(root, ["tsconfig.json"], realContext(root), path.join(root, "logs"), 60, 5);
        assert.equal(r.status, "match");
        assert.equal(r.strict_status, "match");
        assert.equal(r.configs[0].tsgo.exit, 2);
        for (const label of ["tsgo", "tsrs", "tsgo-strict", "tsrs-strict"])
            assert(read(path.join(root, `logs/0/${label}.log`)).includes("{ status: string; user?: undefined; }"));
        assert(!fs.existsSync(path.join(root, "logs/0/diagnostics.diff")));
        assert(!fs.existsSync(path.join(root, "logs/0/diagnostics-strict.diff")));
    }),
);
test("default checkers and strict reference remain independent", realOptions, () =>
    temporary(async (root) => {
        writeJSON(path.join(root, "tsconfig.json"), {
            compilerOptions: { strict: true, target: "es2022", types: [], checkers: 8 },
            files: ["a.ts", "b.ts"],
        });
        write(
            path.join(root, "a.ts"),
            'type D = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9";\ntype U = `${D}${D}${D}${D}`;\ntype Chain<T, N extends unknown[] = []> = N["length"] extends 100 ? { value: T } : T extends string ? Chain<T, [...N, 0]> : never;\ndeclare function make<T>(f: (x: T) => void): Chain<T>;\nconst big = make((x: U) => {});\n',
        );
        write(path.join(root, "b.ts"), "declare function id<T>(x: T): T;\nconst y = id(big);\n");
        const r = await checkProjects(root, ["tsconfig.json"], realContext(root), path.join(root, "logs"), 60, 5),
            depth = "error TS2589: Type instantiation is excessively deep and possibly infinite.",
            go = read(path.join(root, "logs/0/tsgo.log")),
            strict = read(path.join(root, "logs/0/tsgo-strict.log"));
        assert(!go.split("\n")[0].includes("--checkers"));
        assert.deepEqual(
            go.split("\n").filter((l) => l.includes(depth)),
            ["a.ts(5,13): " + depth, "b.ts(2,11): " + depth],
        );
        assert.deepEqual(
            strict.split("\n").filter((l) => l.includes(depth)),
            ["a.ts(5,13): " + depth],
        );
        assert(read(path.join(root, "logs/0/tsrs-strict.log")).split("\n")[0].includes("--noLazyMembers"));
        assert.equal(r.configs[0].strict.status, "match");
        assert.equal(
            r.configs[0].status,
            read(path.join(root, "logs/0/tsrs.log")).includes("b.ts(2,11): " + depth) ? "match" : "mismatch",
        );
    }),
);
test("strict mode stays within the check budget", realOptions, () =>
    temporary(async (root) => {
        const ctx = realContext(root),
            hang = path.join(root, "tsrs-hangs-in-strict-mode");
        write(
            hang,
            `#!/bin/sh\ncase "$*" in *noLazyMembers*) sleep 60;; esac\nexec '${ctx.tsrs.replaceAll("'", "'\\''")}' "$@"\n`,
        );
        fs.chmodSync(hang, 0o755);
        const configs = ["one", "two", "three"].map((n) => {
            writeJSON(path.join(root, n, "tsconfig.json"), { files: ["a.ts"] });
            write(path.join(root, n, "a.ts"), "export const x: number = 1;\n");
            return n + "/tsconfig.json";
        });
        for (const [budget, selected, expected] of [
            [6, configs, "budget_exhausted"],
            [3, configs.slice(0, 1), "tsrs_failed"],
        ]) {
            const start = performance.now(),
                r = await checkProjects(
                    root,
                    selected,
                    { ...ctx, tsrs: hang },
                    path.join(root, "logs" + budget),
                    budget,
                    5,
                );
            assert(performance.now() - start < (budget + 1) * 1000);
            assert(r.configs.every((c) => c.status === "match"));
            assert(r.configs.every((c) => c.strict.status === expected));
            assert.equal(r.status, "match");
        }
    }),
);
test("strict reference failure skips strict tsrs", realOptions, () =>
    temporary(async (root) => {
        const ctx = realContext(root),
            broken = path.join(root, "tsgo-fails-with-checkers");
        write(
            broken,
            `#!/bin/sh\ncase "$*" in *--checkers*) exit 3;; esac\nexec '${ctx.tsgo.replaceAll("'", "'\\''")}' "$@"\n`,
        );
        fs.chmodSync(broken, 0o755);
        writeJSON(path.join(root, "tsconfig.json"), { files: ["a.ts"] });
        write(path.join(root, "a.ts"), "export const x: number = 1;\n");
        const r = await checkProjects(
            root,
            ["tsconfig.json"],
            { ...ctx, tsgo: broken },
            path.join(root, "logs"),
            30,
            5,
        );
        assert.equal(r.configs[0].status, "match");
        assert.equal(r.configs[0].strict.status, "reference_failed");
        assert(!r.configs[0].strict.tsrs);
        assert(!fs.existsSync(path.join(root, "logs/0/tsrs-strict.log")));
        assert.equal(r.strict_status, "not_checked");
    }),
);
test("reference timeout does not spend budget on tsrs", realOptions, () =>
    temporary(async (root) => {
        const slow = path.join(root, "slow-tsgo");
        write(slow, "#!/bin/sh\nsleep 30\n");
        fs.chmodSync(slow, 0o755);
        writeJSON(path.join(root, "tsconfig.json"), { files: ["a.ts"] });
        write(path.join(root, "a.ts"), "export {};\n");
        const r = await checkConfig(
            root,
            "tsconfig.json",
            { ...realContext(root), tsgo: slow },
            path.join(root, "logs"),
            0.5,
        );
        assert.equal(r.status, "reference_failed");
        assert(r.tsgo.timeout);
        assert(!r.tsrs);
        assert(!fs.existsSync(path.join(root, "logs/tsrs.log")));
    }),
);
