// Existing real-container isolation controls, ported from unittest.
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { childEnv } from "./run.mjs";
import { checkInContainer, copyLogs, launch } from "./sandbox.mjs";
import { read, json, write, writeJSON, run, sleep } from "../lib/cli.mjs";
const IMAGE = process.env.TSRS_CONFORMANCE_SANDBOX_IMAGE,
    WORKER = fileURLToPath(new URL("run.mjs", import.meta.url)),
    options = { skip: !IMAGE };
function setup(t) {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-sandbox-test-"));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    const compilers = path.join(root, "bin");
    for (const name of ["tsgo", "tsrs"]) {
        const file = path.join(compilers, name);
        write(
            file,
            "#!/usr/bin/env node\nconsole.log(process.cwd()+'/a.ts(1,1): error TS2322: synthetic diagnostic.');console.log('Files: 1');process.exitCode=2;\n",
        );
        fs.chmodSync(file, 0o755);
    }
    return {
        root,
        compilers,
        fixture(manager, extra = {}) {
            const source = fs.mkdtempSync(path.join(root, "source-"));
            writeJSON(path.join(source, "tsconfig.json"), { files: ["a.ts"] });
            write(path.join(source, "a.ts"), "const a: number = 'a';\n");
            if (manager)
                writeJSON(path.join(source, "package.json"), {
                    name: "sandbox-smoke",
                    private: true,
                    packageManager: manager,
                });
            for (const [name, text] of Object.entries(extra)) write(path.join(source, name), text);
            const env = childEnv(path.join(root, "git-home"), path.join(root, "git-cache"));
            for (const args of [
                ["init", "--quiet", "--initial-branch=main", "--template="],
                ["add", "."],
                [
                    "-c",
                    "user.name=Sandbox",
                    "-c",
                    "user.email=sandbox@example.invalid",
                    "commit",
                    "--quiet",
                    "-m",
                    "fixture",
                ],
            ])
                run(["git", ...args], { cwd: source, env });
            return {
                source,
                repo: {
                    repository: "synthetic/sandbox",
                    branch: "main",
                    commit: run(["git", "rev-parse", "HEAD"], { cwd: source, env }).stdout.trim(),
                },
            };
        },
        worker(source, repo, timeout = 90) {
            const work = path.join(root, "work"),
                request = path.join(root, "request.json");
            writeJSON(request, { repo, setup_timeout: 60, check_timeout: 10, max_configs: 5, source: true });
            return {
                ran: launch(
                    IMAGE,
                    WORKER,
                    path.join(compilers, "tsgo"),
                    path.join(compilers, "tsrs"),
                    work,
                    request,
                    path.join(root, "sandbox.log"),
                    timeout,
                    source,
                ),
                work,
            };
        },
        evidence(name, data, output) {
            if (!process.env.TSRS_SANDBOX_EVIDENCE) return;
            const dest = path.join(process.env.TSRS_SANDBOX_EVIDENCE, name);
            writeJSON(path.join(dest, "result.json"), data);
            copyLogs(output, dest);
            if (fs.existsSync(path.join(root, "sandbox.log")))
                fs.copyFileSync(path.join(root, "sandbox.log"), path.join(dest, "sandbox.log"));
        },
    };
}
test("Yarn plugins cannot reach host inputs and detached processes die", options, async (t) => {
    const s = setup(t),
        token = path.join(s.root, "parent-token-marker"),
        cache = path.join(s.root, "host-release-cache");
    write(token, "synthetic-parent-token");
    write(cache, "trusted-cache-marker");
    const compiler = read(path.join(s.compilers, "tsrs")),
        worker = read(WORKER),
        plugin = `const fs=require('fs'),cp=require('child_process');exports.name='sandbox-probe';exports.factory=()=>{const blocked=fn=>{try{fn();return false;}catch{return true;}};const evidence={hostToken:blocked(()=>fs.readFileSync(${JSON.stringify(token)})),parentRoot:blocked(()=>fs.readFileSync('/proc/1/root'+${JSON.stringify(token)})),hostCache:blocked(()=>fs.writeFileSync(${JSON.stringify(cache)},'poisoned')),compiler:blocked(()=>fs.appendFileSync('/inputs/tsrs','poisoned')),harness:blocked(()=>fs.appendFileSync('/harness/tools/repo-conformance/run.mjs','poisoned')),dockerSocket:!fs.existsSync('/var/run/docker.sock'),nonRoot:process.getuid()!==0,noNewPrivileges:/NoNewPrivs:\\s+1/.test(fs.readFileSync('/proc/self/status','utf8')),noCapabilities:/CapEff:\\s+0+\\n/.test(fs.readFileSync('/proc/self/status','utf8')),noRunnerToken:!process.env.ACTIONS_RUNTIME_TOKEN&&!process.env.GITHUB_TOKEN,noSudo:cp.spawnSync('sudo',['-n','true']).status!==0};fs.writeFileSync('/work/out/hook.json',JSON.stringify(evidence));try{fs.symlinkSync(${JSON.stringify(token)},'/work/out/host-link');}catch{}const child=cp.spawn(process.execPath,['-e',"const fs=require('fs');const f='/work/out/heartbeat';fs.appendFileSync(f,'.');setInterval(()=>fs.appendFileSync(f,'.'),20)"],{detached:true,stdio:'ignore'});child.unref();return{};};`;
    const { source, repo } = s.fixture("yarn@4.9.2", {
            ".yarnrc.yml":
                "enableTelemetry: false\nplugins:\n  - path: .yarn/plugins/probe.cjs\n    spec: sandbox-probe\n",
            ".yarn/plugins/probe.cjs": plugin,
            "yarn.lock":
                '# This file is generated by running "yarn install" inside your project.\n# Manual changes might be lost - proceed with caution!\n\n__metadata:\n  version: 8\n  cacheKey: 10c0\n\n"sandbox-smoke@workspace:.":\n  version: 0.0.0-use.local\n  resolution: "sandbox-smoke@workspace:."\n  languageName: unknown\n  linkType: soft\n',
        }),
        { ran, work } = s.worker(source, repo);
    assert.deepEqual(ran, { exit: 0, timeout: false }, read(path.join(s.root, "sandbox.log")));
    const result = json(path.join(work, "result.json")),
        evidence = json(path.join(work, "out/hook.json"));
    s.evidence("yarn-plugin", { result, isolation: evidence }, path.join(work, "out"));
    assert.equal(result.status, "match");
    assert.equal(result.strict_status, "match");
    assert(Object.values(evidence).every(Boolean), JSON.stringify(evidence));
    assert.equal(read(token), "synthetic-parent-token");
    assert.equal(read(cache), "trusted-cache-marker");
    assert.equal(read(path.join(s.compilers, "tsrs")), compiler);
    assert.equal(read(WORKER), worker);
    const heartbeat = path.join(work, "out/heartbeat"),
        before = read(heartbeat);
    assert(before);
    await sleep(300);
    assert.equal(read(heartbeat), before);
    const copied = path.join(s.root, "exported");
    copyLogs(path.join(work, "out"), copied);
    assert(!fs.existsSync(path.join(copied, "host-link")));
});
test("timeout removes detached compiler children", options, async (t) => {
    const s = setup(t);
    write(
        path.join(s.compilers, "tsgo"),
        `#!/usr/bin/env node\nconst cp=require('node:child_process');cp.spawn(process.execPath,['-e',"const fs=require('node:fs');const f='/work/out/heartbeat';fs.appendFileSync(f,'.');setInterval(()=>fs.appendFileSync(f,'.'),20)"],{detached:true,stdio:'ignore'});setTimeout(()=>{},60000);\n`,
    );
    const { source, repo } = s.fixture(),
        { ran, work } = s.worker(source, repo, 3);
    s.evidence("timeout", ran, path.join(work, "out"));
    assert(ran.timeout);
    const file = path.join(work, "out/heartbeat"),
        before = read(file);
    assert(before);
    await sleep(300);
    assert.equal(read(file), before);
});
test("supported managers install inside containers", options, (t) => {
    const s = setup(t);
    for (const manager of [
        "npm@11.6.2",
        "pnpm@10.23.0",
        "pnpm@12.9.1",
        "pnpm@12.10.1",
        "yarn@1.22.22",
        "bun@1.4.3",
        "nub@0.8.3",
    ]) {
        const { source, repo } = s.fixture(manager, {
                ".pnpmfile.cjs": "throw new Error('pnpmfile must not execute');\n",
            }),
            out = path.join(s.root, manager.replace("@", "-")),
            scratch = path.join(s.root, "scratch"),
            result = checkInContainer(
                repo,
                path.join(s.compilers, "tsgo"),
                path.join(s.compilers, "tsrs"),
                out,
                scratch,
                60,
                10,
                5,
                IMAGE,
                source,
            );
        s.evidence(manager, result, out);
        assert.equal(result.status, "match", JSON.stringify(result));
        assert.equal(result.strict_status, "match");
        assert.deepEqual(fs.readdirSync(scratch), []);
        fs.rmSync(source, { recursive: true });
    }
});
test("default differences and strict matches keep logs and statuses", options, (t) => {
    const s = setup(t),
        compiler = path.join(s.compilers, "tsrs");
    write(
        compiler,
        "#!/usr/bin/env node\nconst message=process.argv.includes('--checkerAssignment')?'synthetic diagnostic.':'changed.';console.log(process.cwd()+'/a.ts(1,1): error TS2322: '+message);console.log('Files: 1');process.exitCode=2;\n",
    );
    const { source, repo } = s.fixture(),
        out = path.join(s.root, "modes"),
        result = checkInContainer(
            repo,
            path.join(s.compilers, "tsgo"),
            compiler,
            out,
            path.join(s.root, "scratch"),
            60,
            10,
            5,
            IMAGE,
            source,
        );
    s.evidence("modes", result, out);
    assert.equal(result.status, "mismatch");
    assert.equal(result.strict_status, "match");
    assert.deepEqual(result.configs[0].difference, ["diagnostics"]);
    assert.match(read(path.join(out, "0/diagnostics.diff")), /changed\./);
    assert.match(read(path.join(out, "0/tsrs-strict.log")), /synthetic diagnostic\./);
});
