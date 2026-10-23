import { isDeepStrictEqual } from "node:util";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import {
    ROOT,
    args,
    read,
    write,
    json,
    writeJSON,
    run,
    runAsync,
    walk,
    mapLimit,
    pinnedCommit,
    requireValue,
    required,
    isMain,
    main,
    sleep,
} from "../lib/cli.mjs";
import { unified } from "../lib/diff.mjs";
export const HARNESS = 5;
export const MODES = {
    default: { tsgo: [], tsrs: [] },
    strict: { tsgo: ["--checkers", "4"], tsrs: ["--checkerAssignment", "go", "--checkers", "4", "--noLazyMembers"] },
};
const QUERY = "language:TypeScript stars:>100 archived:false fork:false",
    SHA = /^[0-9a-f]{40}$/,
    COMPARED = ["match", "mismatch", "tsrs_failed"],
    now = () => performance.now() / 1000;
export function childEnv(home, cache) {
    const env = Object.fromEntries(
            Object.entries(process.env).filter(([k]) =>
                ["PATH", "LANG", "LC_ALL", "TMPDIR", "USER", "LOGNAME"].includes(k),
            ),
        ),
        runtime = path.join(home, ".runtime");
    fs.mkdirSync(runtime, { recursive: true, mode: 0o700 });
    for (const name of [".npmrc", ".npmrc-global", ".gitconfig"]) {
        const file = path.join(home, name);
        if (!fs.existsSync(file)) write(file, "");
    }
    return {
        ...env,
        HOME: home,
        XDG_RUNTIME_DIR: runtime,
        XDG_CONFIG_HOME: path.join(home, ".config"),
        XDG_STATE_HOME: path.join(home, ".state"),
        XDG_CACHE_HOME: path.join(cache, "xdg"),
        XDG_DATA_HOME: path.join(cache, "data"),
        npm_config_userconfig: path.join(home, ".npmrc"),
        npm_config_globalconfig: path.join(home, ".npmrc-global"),
        npm_config_cache: path.join(cache, "npm"),
        COREPACK_HOME: path.join(cache, "corepack"),
        BUN_INSTALL_CACHE_DIR: path.join(cache, "bun"),
        GIT_CONFIG_GLOBAL: path.join(home, ".gitconfig"),
        GIT_CONFIG_NOSYSTEM: "1",
        CI: "true",
        GIT_TERMINAL_PROMPT: "0",
        COREPACK_ENABLE_DOWNLOAD_PROMPT: "0",
        COREPACK_ENABLE_PROJECT_SPEC: "0",
        npm_config_ignore_scripts: "true",
        YARN_ENABLE_SCRIPTS: "false",
        YARN_NODE_LINKER: "node-modules",
        YARN_IGNORE_PATH: "1",
    };
}
export async function execute(command, cwd, log, timeout, env) {
    write(log, "$ " + command.join(" ") + "\n");
    const fd = fs.openSync(log, "a"),
        started = now(),
        child = spawn(command[0], command.slice(1), { cwd, env, stdio: ["ignore", fd, fd], detached: true });
    let timedOut = false;
    const timer = setTimeout(
        () => {
            timedOut = true;
            try {
                process.kill(-child.pid, "SIGKILL");
            } catch (e) {
                if (e.code !== "ESRCH") throw e;
            }
        },
        Math.max(0.01, timeout) * 1000,
    );
    try {
        const code = await new Promise((resolve, reject) => {
            child.once("error", reject);
            child.once("close", (code) => resolve(code ?? -9));
        });
        return { exit: code, timeout: timedOut, seconds: Number((now() - started).toFixed(3)) };
    } finally {
        clearTimeout(timer);
        fs.closeSync(fd);
    }
}
export function checkVersion(name, version) {
    const [base, integrity, ...extra] = version.split("+");
    requireValue(
        !extra.length &&
            /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(?:0|[1-9]\d*|\d*[A-Za-z-][\dA-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][\dA-Za-z-]*))*)?$/.test(
                base,
            ) &&
            (!integrity ||
                (["npm", "pnpm", "yarn"].includes(name) &&
                    [224, 256, 384, 512].some((bits) =>
                        new RegExp(`^sha${bits}\\.[0-9a-fA-F]{${bits / 4}}$`).test(integrity),
                    ))),
        `Invalid package manager version: ${name}@${version}`,
    );
}
export function declaredManager(pkg) {
    let spec = pkg.packageManager;
    const engine = pkg.devEngines?.packageManager;
    if (!spec && engine && /^\d+\.\d+\.\d+$/.test(engine.version)) spec = `${engine.name}@${engine.version}`;
    if (!spec) return null;
    requireValue(typeof spec === "string" && spec.includes("@"), `Invalid packageManager: ${spec}`);
    const i = spec.indexOf("@"),
        name = spec.slice(0, i),
        version = spec.slice(i + 1);
    checkVersion(name, version);
    return [name, version];
}
export function defaultPnpm(root, pkg) {
    const lock = path.join(root, "pnpm-lock.yaml"),
        major = Number(fs.existsSync(lock) ? (read(lock).match(/^lockfileVersion: '?(\d+)/)?.[1] ?? 9) : 9);
    return major <= 5
        ? "7.33.7"
        : major === 6
          ? "8.15.9"
          : pkg.pnpm || (pkg.workspaces && !fs.existsSync(path.join(root, "pnpm-workspace.yaml")))
            ? "10.33.0"
            : "12.10.1";
}
export function installPlan(root) {
    const file = path.join(root, "package.json");
    if (!fs.existsSync(file)) return null;
    const pkg = json(file),
        locks = {
            bun: ["bun.lock", "bun.lockb"],
            pnpm: ["pnpm-lock.yaml"],
            yarn: ["yarn.lock"],
            npm: ["package-lock.json", "npm-shrinkwrap.json"],
        },
        locked = Object.keys(locks).filter((k) => locks[k].some((f) => fs.existsSync(path.join(root, f))));
    let [name, version] = declaredManager(pkg) ?? ["", ""];
    if (name !== "nub" && locked.length && !locked.includes(name)) [name, version] = [locked[0], ""];
    name ||= "npm";
    const lock = name === "nub" ? Boolean(locked.length) : locked.includes(name),
        frozen = lock ? ["--frozen-lockfile"] : [];
    let command;
    if (name === "pnpm")
        command = [
            "corepack",
            `pnpm@${version || defaultPnpm(root, pkg)}`,
            "install",
            ...(lock ? frozen : ["--no-frozen-lockfile"]),
            "--ignore-scripts",
            "--ignore-pnpmfile",
            "--config.engine-strict=false",
        ];
    else if (name === "yarn") {
        version ||= lock && read(path.join(root, "yarn.lock")).includes("__metadata:") ? "4.9.2" : "1.22.22";
        command = [
            "corepack",
            `yarn@${version}`,
            "install",
            ...(Number(version.split(".")[0]) >= 2
                ? [...(lock ? ["--immutable"] : []), "--mode=skip-build"]
                : [
                      ...(lock ? frozen : ["--no-lockfile"]),
                      "--ignore-scripts",
                      "--non-interactive",
                      "--ignore-engines",
                      "--ignore-platform",
                  ]),
        ];
    } else if (name === "npm")
        command = [
            ...(version ? ["corepack", `npm@${version}`] : ["npm"]),
            ...(lock ? ["ci"] : ["install", "--package-lock=false"]),
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            "--engine-strict=false",
        ];
    else if (name === "bun") {
        version ||= "1.4.3";
        command = ["bun", "install", ...frozen, "--ignore-scripts"];
    } else if (name === "nub")
        command = ["nub", "install", ...(lock ? frozen : ["--no-frozen-lockfile"]), "--ignore-scripts"];
    else throw new Error(`Unsupported package manager: ${name}@${version}`);
    return {
        manager: name,
        command,
        dependencies: lock ? "lockfile" : "unpinned",
        ...(["bun", "nub"].includes(name) ? { tool: [name, version] } : {}),
    };
}
export const failed = (r) => r.timeout || ![0, 1, 2].includes(r.exit);
export const compare = (go, rs) =>
    failed(go)
        ? "reference_failed"
        : failed(rs)
          ? "tsrs_failed"
          : ["exit", "files", "diagnostics"].some((k) => go[k] !== rs[k])
            ? "mismatch"
            : go.files
              ? "match"
              : "no_inputs";
export function splitOutput(text) {
    const diagnostics = [],
        other = [];
    let inside = false;
    for (const line of text.split(/\r?\n/)) {
        if (/^(?:.+\(\d+,\d+\): )?(?:error|warning|message) TS\d+: /.test(line)) {
            diagnostics.push(line);
            inside = true;
        } else if (inside && line.startsWith("  ")) diagnostics[diagnostics.length - 1] += "\n" + line;
        else {
            other.push(line);
            inside = false;
        }
    }
    return [diagnostics, other];
}
export function difference(go, rs) {
    const a = splitOutput(go.diagnostics),
        b = splitOutput(rs.diagnostics);
    return [
        ...(go.exit !== rs.exit ? ["exit code"] : []),
        ...(go.files !== rs.files ? ["loaded files"] : []),
        ...["diagnostics", "other output"].filter((_, i) => JSON.stringify(a[i]) !== JSON.stringify(b[i])),
    ];
}
const summary = ({ diagnostics, ...rest }) => rest;
async function compile(root, config, ctx, out, label, binary, flags, timeout) {
    fs.mkdirSync(out, { recursive: true });
    const info = path.join(out, label + ".tsbuildinfo"),
        log = path.join(out, label + ".log");
    fs.rmSync(info, { force: true });
    const result = await execute(
        [
            binary,
            "-p",
            config,
            ...flags,
            "--noEmit",
            "--pretty",
            "false",
            "--extendedDiagnostics",
            "--incremental",
            "--tsBuildInfoFile",
            info,
        ],
        root,
        log,
        timeout,
        ctx.env,
    );
    fs.rmSync(info, { force: true });
    const text = read(log)
            .slice(read(log).indexOf("\n") + 1)
            .replaceAll("\r\n", "\n"),
        m = /^Files:\s+(\d+)\s*$/m.exec(text);
    return {
        ...result,
        files: Number(m?.[1] ?? 0),
        diagnostics: (m ? text.slice(0, m.index) : text)
            .replaceAll(root, "<repo>")
            .replaceAll(path.dirname(binary), "<compiler>")
            .trimEnd(),
        limit: Number(timeout.toFixed(1)),
    };
}
function outcome(go, rs, diff) {
    const status = rs ? compare(go, rs) : "reference_failed";
    if (status !== "mismatch") return { status };
    write(
        diff,
        unified(
            `exit: ${go.exit}\nfiles: ${go.files}\n${go.diagnostics}\n`,
            `exit: ${rs.exit}\nfiles: ${rs.files}\n${rs.diagnostics}\n`,
            "tsgo",
            "tsrs",
        ),
    );
    return { status, difference: difference(go, rs) };
}
export async function checkConfig(root, config, ctx, out, timeout) {
    out = path.resolve(out);
    const go = await compile(root, config, ctx, out, "tsgo", ctx.tsgo, [], timeout),
        rs = failed(go) ? null : await compile(root, config, ctx, out, "tsrs", ctx.tsrs, [], timeout);
    return {
        config,
        ...outcome(go, rs, path.join(out, "diagnostics.diff")),
        limit: Number(timeout.toFixed(1)),
        tsgo: summary(go),
        ...(rs ? { tsrs: summary(rs) } : {}),
    };
}
async function strict(root, config, ctx, out, budget) {
    const end = now() + budget,
        go = await compile(
            root,
            config.config,
            ctx,
            out,
            "tsgo-strict",
            ctx.tsgo,
            MODES.strict.tsgo,
            Math.min(60, budget / 2),
        ),
        rs = failed(go)
            ? null
            : await compile(
                  root,
                  config.config,
                  ctx,
                  out,
                  "tsrs-strict",
                  ctx.tsrs,
                  MODES.strict.tsrs,
                  Math.min(60, Math.max(0.01, end - now())),
              ),
        result = {
            ...outcome(go, rs, path.join(out, "diagnostics-strict.diff")),
            tsgo: summary(go),
            ...(rs ? { tsrs: summary(rs) } : {}),
        };
    if ([go, rs].some((r) => r?.timeout && r.limit < config.limit)) result.status = "budget_exhausted";
    return result;
}
export async function projectReferences(root, config, ctx, log, timeout) {
    const result = await execute([ctx.tsgo, "-p", config, "--showConfig"], root, log, timeout, ctx.env);
    if (result.exit || result.timeout) return [];
    let refs;
    try {
        refs = JSON.parse(read(log).slice(read(log).indexOf("\n") + 1)).references ?? [];
    } catch {
        return [];
    }
    return refs
        .map((r) => {
            let p = path.resolve(root, path.dirname(config), r.path ?? "");
            if (!p.endsWith(".json")) p = path.join(p, "tsconfig.json");
            return path.relative(root, p);
        })
        .filter((p) => !path.isAbsolute(p) && !p.startsWith("..") && fs.existsSync(path.join(root, p)));
}
const aggregate = (configs, queue = []) =>
    configs.some((c) => c.status === "mismatch")
        ? "mismatch"
        : configs.some((c) => c.status === "tsrs_failed")
          ? "tsrs_failed"
          : configs.some((c) => c.status === "match")
            ? queue.length || configs.some((c) => !["match", "references"].includes(c.status))
                ? "partial"
                : "match"
            : "not_checked";
export async function checkProjects(root, configs, ctx, out, timeout, maxConfigs) {
    const checked = [];
    let queue = [...configs];
    const end = now() + timeout;
    while (
        queue.length &&
        checked.length < 2 * maxConfigs &&
        checked.filter((c) => !["no_inputs", "references"].includes(c.status)).length < maxConfigs
    ) {
        const config = queue.shift(),
            remaining = end - now(),
            index = path.join(out, String(checked.length));
        if (remaining < 1) {
            checked.push({ config, status: "budget_exhausted" });
            break;
        }
        const result = await checkConfig(root, config, ctx, index, Math.min(60, remaining / 2));
        if (result.status === "no_inputs") {
            const done = new Set([...checked.map((c) => c.config), config]),
                refs = (
                    await projectReferences(
                        root,
                        config,
                        ctx,
                        path.join(index, "references.log"),
                        Math.min(10, Math.max(0.01, end - now())),
                    )
                ).filter((r) => !done.has(r));
            if (refs.length) {
                result.status = "references";
                result.references = refs;
                queue = [...refs, ...queue.filter((q) => !refs.includes(q))];
            }
        }
        checked.push(result);
    }
    const status = aggregate(checked, queue),
        pending = checked.map((c, i) => [c, i]).filter(([c]) => COMPARED.includes(c.status));
    for (let n = 0; n < pending.length; n++) {
        const [c, i] = pending[n],
            remaining = end - now();
        c.strict =
            remaining >= 1
                ? await strict(root, c, ctx, path.join(out, String(i)), remaining / (pending.length - n))
                : { status: "budget_exhausted" };
    }
    return { status, strict_status: aggregate(pending.map(([c]) => c.strict)), configs: checked };
}
export async function checkRepo(repo, tsgo, tsrs, out, cache, setupTimeout, checkTimeout, maxConfigs, source) {
    const result = { ...repo, status: "setup_failed", configs: [] },
        temp = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-repo-")),
        root = path.join(temp, "repo"),
        ctx = { tsgo, tsrs, env: childEnv(path.join(temp, "home"), cache) },
        end = now() + setupTimeout;
    try {
        const url = source ?? `https://github.com/${repo.repository}.git`;
        let cloned;
        if (repo.commit) {
            fs.mkdirSync(root);
            cloned = await execute(["git", "init", "-q", root], temp, path.join(out, "clone.log"), 60, ctx.env);
            if (!cloned.exit)
                cloned = await execute(
                    ["git", "-C", root, "fetch", "-q", "--depth", "1", url, repo.commit],
                    temp,
                    path.join(out, "clone.log"),
                    Math.min(60, setupTimeout),
                    ctx.env,
                );
            if (!cloned.exit)
                cloned = await execute(
                    ["git", "-C", root, "checkout", "-q", "FETCH_HEAD"],
                    temp,
                    path.join(out, "clone.log"),
                    60,
                    ctx.env,
                );
        } else
            cloned = await execute(
                ["git", "clone", "--depth", "1", "--single-branch", "--branch", repo.branch, url, root],
                temp,
                path.join(out, "clone.log"),
                Math.min(60, setupTimeout),
                ctx.env,
            );
        if (cloned.exit || cloned.timeout) {
            result.reason = "Clone failed or timed out; see clone.log";
            return result;
        }
        result.commit = run(["git", "rev-parse", "HEAD"], { cwd: root, env: ctx.env }).stdout.trim();
        const configs = run(["git", "ls-files", "-z"], { cwd: root, env: ctx.env })
            .stdout.split("\0")
            .filter((p) => path.basename(p) === "tsconfig.json" && !p.split("/").includes("node_modules"))
            .sort((a, b) => a.split("/").length - b.split("/").length || a.localeCompare(b));
        result.available_configs = configs.length;
        if (!configs.length) return { ...result, status: "no_config", reason: "No tracked tsconfig.json" };
        const plan = installPlan(root);
        if (plan) {
            result.dependencies = plan.dependencies;
            let command = [...plan.command];
            if (plan.tool) {
                const [name, version] = plan.tool;
                checkVersion(name, version);
                const arch = name === "bun" && process.arch === "arm64" ? "aarch64" : process.arch,
                    pkg = `${name === "bun" ? "@oven" : "@nubjs"}/${name}-${process.platform}-${arch}`,
                    prefix = path.join(cache, "tools", `${pkg}@${version}`.replace(/[^A-Za-z0-9.-]/g, "_")),
                    binary = path.join(prefix, "node_modules", pkg, "bin", name);
                fs.mkdirSync(prefix, { recursive: true });
                const installed = await execute(
                    [
                        "npm",
                        "install",
                        "--prefix",
                        prefix,
                        "--no-save",
                        "--package-lock=false",
                        "--ignore-scripts",
                        "--no-audit",
                        "--no-fund",
                        `${pkg}@${version}`,
                    ],
                    prefix,
                    path.join(out, "tool.log"),
                    Math.max(0.01, end - now()),
                    ctx.env,
                );
                requireValue(
                    !installed.exit && !installed.timeout && fs.existsSync(binary),
                    `Could not install ${pkg}@${version}`,
                );
                fs.chmodSync(binary, 0o755);
                command[0] = binary;
            }
            if (end <= now()) return { ...result, reason: "Dependency setup budget exhausted" };
            const installed = await execute(command, root, path.join(out, "install.log"), end - now(), ctx.env);
            result.install = { ...installed, command: plan.command.join(" ") };
            if (installed.exit || installed.timeout)
                return {
                    ...result,
                    reason: `Dependency installation failed or timed out (${plan.manager}); see install.log`,
                };
        }
        return { ...result, ...(await checkProjects(root, configs, ctx, out, checkTimeout, maxConfigs)) };
    } catch (error) {
        return { ...result, reason: error.message };
    } finally {
        fs.rmSync(temp, { recursive: true, force: true });
    }
}
export function parseShards(text, count) {
    requireValue(/^\s*\d+\s*(,\s*\d+\s*)*$/.test(text), `Shards must be comma-separated indexes, not ${text}`);
    const shards = [...new Set(text.split(",").map(Number))].sort((a, b) => a - b);
    requireValue(shards.at(-1) < count, `Shards must be between 0 and ${count - 1}`);
    return shards;
}
export function checkDispatch(runID, shards) {
    requireValue(Boolean(runID) === Boolean(shards), "Set rerun_run_id and rerun_shards together");
    if (runID) requireValue(/^[1-9]\d{0,19}$/.test(runID), `rerun_run_id must be a workflow run id, not ${runID}`);
    if (shards) parseShards(shards, 20);
}
export function checkRerun(manifest, shards, earlier, inputs) {
    requireValue(!manifest.recovery_of_run_id, `Run is a recovery; use original run ${manifest.recovery_of_run_id}`);
    requireValue(
        manifest.harness === HARNESS,
        `The manifest is from harness version ${manifest.harness}, not ${HARNESS}`,
    );
    requireValue(
        manifest.repositories.every((r) => SHA.test(r.commit)) &&
            [manifest.tsrs_commit, manifest.typescript_commit].every((s) => SHA.test(s)),
        "The manifest does not pin repository and compiler commits",
    );
    requireValue(
        earlier.path?.split("@")[0] === ".github/workflows/repo-conformance.yml",
        "Not a repository conformance run",
    );
    requireValue(
        earlier.head_repository?.full_name && earlier.head_repository.full_name === earlier.repository?.full_name,
        "Run was built from a different repository",
    );
    requireValue(
        earlier.event === "pull_request" || earlier.head_sha === manifest.tsrs_commit,
        "Run commit does not match manifest",
    );
    for (const name of ["tsgo", "tsrs"])
        requireValue(fs.statSync(path.join(inputs, "bin", name)).isFile(), `The run inputs have no ${name} binary`);
    return parseShards(shards, manifest.shards);
}
const same = isDeepStrictEqual;
const atPin = (r, pins) =>
    !pins[r.repository] ||
    r.commit === pins[r.repository] ||
    (!r.commit && r.status === "setup_failed" && !r.configs.length);
export function differences(r) {
    const found = (r.configs ?? []).flatMap((c) =>
        [
            ["default", c],
            ["strict", c.strict ?? {}],
        ]
            .filter(([, e]) => ["mismatch", "tsrs_failed"].includes(e.status))
            .map(([mode, e]) => `${c.config}: ${mode} ${e.status}`),
    );
    return found.length
        ? found
        : [
              ["default", r.status],
              ["strict", r.strict_status],
          ]
              .filter(([, s]) => ["mismatch", "tsrs_failed"].includes(s))
              .map(([m, s]) => `${m} ${s}`);
}
export function loadShards(directory, manifest, errors) {
    const reports = new Map();
    if (!fs.existsSync(directory)) return reports;
    for (const file of [...walk(directory, (f) => /^shard-.*\.json$/.test(path.basename(f)))].sort()) {
        const r = json(file);
        r.modes ??= { default: MODES.default };
        const shard = r.shard;
        if (!Number.isInteger(shard) || shard < 0 || shard >= manifest.shards || reports.has(shard)) {
            errors.push(`${file}: duplicate or unexpected shard ${shard}`);
            continue;
        }
        if (["tsrs_commit", "typescript_commit"].some((k) => r[k] !== manifest[k])) {
            errors.push(`${file}: compiler commits do not match the manifest; its results are left out`);
            continue;
        }
        if (!same(r.modes, MODES) && !same(r.modes, { default: MODES.default })) {
            errors.push(`${file}: unknown comparison modes; its results are left out`);
            continue;
        }
        const expected = new Set(
                manifest.repositories.filter((_, i) => i % manifest.shards === shard).map((r) => r.repository),
            ),
            outside = r.results.filter((r) => !expected.has(r.repository));
        if (outside.length)
            errors.push(
                `${file}: results for repositories outside shard ${shard}: ${outside.map((r) => r.repository).join(", ")}`,
            );
        r.results = r.results.filter((r) => expected.has(r.repository));
        reports.set(shard, { ...r, path: file });
    }
    return reports;
}
export function replaceShards(reports, earlier, manifest, pins, errors) {
    for (const [shard, report] of earlier) {
        if (!reports.has(shard)) {
            reports.set(shard, report);
            continue;
        }
        const assigned = manifest.repositories.filter((_, i) => i % manifest.shards === shard).map((r) => r.repository),
            present = new Set(report.results.map((r) => r.repository));
        if (assigned.every((n) => present.has(n))) {
            errors.push(
                `Shard ${shard} was complete in the earlier run (${report.path}); a rerun replaces only lost or partial shards, so the earlier report is kept`,
            );
            reports.set(shard, report);
            continue;
        }
        for (const r of report.results)
            if (atPin(r, pins) && differences(r).length)
                errors.push(
                    `${r.repository}: the earlier run's shard ${shard} recorded ${differences(r).join(", ")}; a rerun does not clear it`,
                );
    }
}
export function summarize(manifest, directory, out, previous) {
    const pins = Object.fromEntries(manifest.repositories.map((r) => [r.repository, r.commit])),
        errors = [],
        reports = loadShards(directory, manifest, errors),
        current = new Map(reports);
    if (previous) replaceShards(reports, loadShards(previous, manifest, errors), manifest, pins, errors);
    if (reports.size) {
        const base = Math.min(...(current.size ? current : reports).keys()),
            reference = reports.get(base);
        for (const [shard, report] of reports) {
            const changed = ["setup_timeout", "check_timeout", "max_configs", "modes"].filter(
                (k) => !same(report[k], reference[k]),
            );
            if (changed.length) {
                errors.push(`Shard ${shard} used other settings: ${changed.join(", ")}; its results are left out`);
                reports.delete(shard);
            }
        }
    }
    const results = new Map();
    for (const report of reports.values())
        for (const r of report.results) {
            if (results.has(r.repository)) errors.push(`Duplicate result for ${r.repository}`);
            else if (!atPin(r, pins))
                errors.push(`${r.repository}: checked ${r.commit}, manifest pins ${pins[r.repository]}`);
            else results.set(r.repository, r);
        }
    for (const name of Object.keys(pins).sort())
        if (!results.has(name)) errors.push(`Missing repository result: ${name}`);
    if (reports.size !== manifest.shards) errors.push("Some shard reports are missing");
    const configs = [...results.values()].flatMap((r) => r.configs),
        completed = configs.filter((c) => ["match", "mismatch"].includes(c.status)).length,
        count = (items, status) => items.filter((i) => i.status === status).length,
        eligible = configs.filter((c) => COMPARED.includes(c.status)),
        strict = eligible.map((c) => c.strict).filter(Boolean),
        clean = configs.filter((c) => c.status === "match" && c.tsgo.exit === 0).length;
    if (!completed) errors.push("No project completed a comparison");
    const lines = [
        "# Top repository conformance",
        "",
        `tsrs \`${manifest.tsrs_commit}\` vs tsgo from TypeScript \`${manifest.typescript_commit}\`.`,
        "",
        "## Default mode",
        "",
        `${results.size} of ${manifest.repositories.length} repositories have a result.`,
        "",
        `Both compilers completed on ${completed} projects: ${count(configs, "match")} identical (${clean} without errors, ${count(configs, "match") - clean} with the same errors) and ${count(configs, "mismatch")} different. tsrs crashed or timed out on ${count(configs, "tsrs_failed")} more after tsgo completed; tsgo failed or timed out on ${count(configs, "reference_failed")}.`,
        "",
        "Identical means the same output in the same order, the same exit code and the same nonzero number of loaded files.",
        "",
        "## Strict mode",
        "",
        `Of the ${eligible.length} projects where tsgo completed in default mode: ${count(strict, "match")} identical, ${count(strict, "mismatch")} different, ${count(strict, "tsrs_failed")} tsrs failures in strict mode; strict tsgo failed on ${count(strict, "reference_failed")} and ${count(strict, "budget_exhausted")} were not run for lack of time. ${eligible.filter((c) => c.status !== "match" && c.strict?.status === "match").length} projects that differ or fail in default mode are identical in strict mode.`,
        "",
        "Strict-mode identity does not excuse a default-mode difference.",
        "",
        "## Repositories",
        "",
        "| Repository | Commit | Status | Strict | Identical / completed projects | Tracked tsconfig.json | Detail |",
        "| --- | --- | --- | --- | --- | --- | --- |",
    ];
    for (const name of Object.keys(pins).sort()) {
        const r = results.get(name) ?? { status: "missing", configs: [] },
            detail =
                (r.reason ??
                    r.configs
                        .filter(
                            (c) =>
                                !["match", "references"].includes(c.status) ||
                                (c.strict && c.strict.status !== "match"),
                        )
                        .map(
                            (c) =>
                                `${c.config}: ${c.status}${c.difference ? ` (${c.difference.join(", ")})` : ""}${c.strict ? `; strict ${c.strict.status}` : ""}`,
                        )
                        .join(", ")) + (r.dependencies === "unpinned" ? "; no lockfile; dependencies unpinned" : "");
        lines.push(
            `| [${name}](https://github.com/${name}) | \`${r.commit ?? pins[name] ?? ""}\` | ${r.status} | ${r.strict_status ?? ""} | ${count(r.configs, "match")} / ${r.configs.filter((c) => ["match", "mismatch"].includes(c.status)).length} | ${r.available_configs ?? 0} | ${detail.replaceAll("|", "\\|").replaceAll("\n", " ").slice(0, 500)} |`,
        );
    }
    if (errors.length) lines.push("", "## Incomplete run", "", ...errors.map((e) => "- " + e));
    write(out, lines.join("\n") + "\n");
    return errors.length
        ? 2
        : [...configs, ...strict].some((c) => ["mismatch", "tsrs_failed"].includes(c.status))
          ? 3
          : 0;
}
function matrix(shards) {
    const value = JSON.stringify({ shard: shards });
    if (process.env.GITHUB_OUTPUT) fs.appendFileSync(process.env.GITHUB_OUTPUT, "matrix=" + value + "\n");
    console.log(value);
}
async function discover(count, shards, out) {
    const headers = { Accept: "application/vnd.github+json", "User-Agent": "tsrs-repo-conformance" },
        token = process.env.GH_TOKEN ?? process.env.GITHUB_TOKEN;
    if (token) headers.Authorization = `Bearer ${token}`;
    const repos = new Map();
    for (let page = 1; page <= 10 && repos.size < count; page++) {
        let data;
        for (let attempt = 1; attempt <= 5; attempt++) {
            const response = await fetch(
                "https://api.github.com/search/repositories?" +
                    new URLSearchParams({
                        q: QUERY,
                        sort: "stars",
                        order: "desc",
                        per_page: "100",
                        page: String(page),
                    }),
                { headers, signal: AbortSignal.timeout(60000) },
            );
            requireValue(response.ok, `GitHub HTTP ${response.status}`);
            data = await response.json();
            if (!data.incomplete_results) break;
            requireValue(
                attempt < 5,
                `GitHub returned an incomplete repository search for page ${page} after ${attempt} attempts`,
            );
            const delay = 5 * 2 ** (attempt - 1);
            console.error(
                `GitHub returned an incomplete repository search for page ${page} (attempt ${attempt}/5); retrying in ${delay}s`,
            );
            await sleep(delay * 1000);
        }
        for (const r of data.items) {
            requireValue(/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(r.full_name), "Invalid repository name");
            repos.set(r.full_name, { repository: r.full_name, branch: r.default_branch, stars: r.stargazers_count });
        }
        if (!data.items.length) break;
    }
    requireValue(repos.size >= count, `Expected ${count} repositories, found only ${repos.size}`);
    const selected = [...repos.values()].slice(0, count),
        temp = fs.mkdtempSync(path.join(os.tmpdir(), "tsrs-plan-"));
    try {
        const env = childEnv(path.join(temp, "home"), path.join(temp, "cache"));
        await mapLimit(selected, 16, async (r) => {
            const result = await runAsync(
                ["git", "ls-remote", `https://github.com/${r.repository}.git`, `refs/heads/${r.branch}`],
                { env, timeout: 60 },
            );
            const sha = result.stdout.trim().split(/\s+/)[0];
            requireValue(SHA.test(sha), `Could not resolve ${r.repository} ${r.branch}`);
            r.commit = sha;
        });
    } finally {
        fs.rmSync(temp, { recursive: true, force: true });
    }
    const manifest = {
        created_at: new Date().toISOString(),
        query: QUERY,
        tsrs_commit: run(["git", "rev-parse", "HEAD"], { cwd: ROOT }).stdout.trim(),
        typescript_commit: pinnedCommit(),
        harness: HARNESS,
        shards: Math.min(shards, count),
        repositories: selected,
    };
    writeJSON(out, manifest);
    return manifest;
}
if (isMain(import.meta.url))
    main(async () => {
        const command = process.argv[2],
            spec = {
                plan: { count: 1000, shards: 10, out: null },
                dispatch: { "run-id": "", shards: "" },
                rerun: { manifest: null, shards: null, run: null },
                run: {
                    manifest: null,
                    shard: 0,
                    tsgo: null,
                    tsrs: null,
                    out: null,
                    cache: null,
                    "sandbox-image": "tsrs-repo-conformance",
                    "setup-timeout": 180,
                    "check-timeout": 120,
                    "max-configs": 5,
                },
                report: { manifest: null, results: null, out: null, previous: null },
                _repo: {},
            };
        if (["--help", "-h"].includes(command) || !command) {
            console.log("Repository conformance commands: plan, dispatch, rerun, run, report. Use <command> --help.");
            return;
        }
        requireValue(command in spec, `unknown command ${command}`);
        const a = args(`Repository conformance: ${command}`, spec[command], process.argv.slice(3));
        if (command === "plan") {
            required(a, "out");
            requireValue(
                Number.isInteger(a.count) &&
                    a.count >= 1 &&
                    a.count <= 1000 &&
                    Number.isInteger(a.shards) &&
                    a.shards >= 1 &&
                    a.shards <= 20,
                "At most 1000 repositories and 20 shards; counts must be positive integers",
            );
            const manifest = await discover(a.count, a.shards, a.out);
            matrix(Array.from({ length: manifest.shards }, (_, i) => i));
        } else if (command === "dispatch") checkDispatch(a["run-id"], a.shards);
        else if (command === "rerun") {
            required(a, "manifest", "shards", "run");
            const manifest = json(a.manifest),
                earlier = json(a.run),
                shards = checkRerun(manifest, a.shards, earlier, path.dirname(a.manifest));
            writeJSON(a.manifest, { ...manifest, recovery_of_run_id: earlier.id });
            matrix(shards);
        } else if (command === "report") {
            required(a, "manifest", "results", "out");
            process.exitCode = summarize(json(a.manifest), a.results, a.out, a.previous);
        } else if (command === "_repo") {
            const r = json("/request.json");
            writeJSON(
                "/work/result.json",
                await checkRepo(
                    r.repo,
                    "/inputs/tsgo",
                    "/inputs/tsrs",
                    "/work/out",
                    "/work/cache",
                    r.setup_timeout,
                    r.check_timeout,
                    r.max_configs,
                    r.source ? "/source" : undefined,
                ),
            );
        } else {
            required(a, "manifest", "tsgo", "tsrs", "out", "cache");
            const manifest = json(a.manifest);
            requireValue(
                Number.isInteger(a.shard) && a.shard >= 0 && a.shard < manifest.shards,
                "Shard outside manifest range",
            );
            for (const k of ["setup-timeout", "check-timeout", "max-configs"])
                requireValue(Number.isInteger(a[k]) && a[k] > 0, `--${k} must be positive`);
            for (const bin of [a.tsgo, a.tsrs]) fs.accessSync(bin, fs.constants.X_OK);
            const { checkInContainer } = await import("./sandbox.mjs"),
                results = [],
                report = {
                    tsrs_commit: manifest.tsrs_commit,
                    typescript_commit: manifest.typescript_commit,
                    shard: a.shard,
                    setup_timeout: a["setup-timeout"],
                    check_timeout: a["check-timeout"],
                    max_configs: a["max-configs"],
                    modes: MODES,
                    results,
                },
                file = path.join(a.out, `shard-${a.shard}.json`);
            writeJSON(file, report);
            for (const repo of manifest.repositories.filter((_, i) => i % manifest.shards === a.shard)) {
                const result = checkInContainer(
                    repo,
                    path.resolve(a.tsgo),
                    path.resolve(a.tsrs),
                    path.resolve(a.out, repo.repository.replace("/", "--")),
                    path.resolve(a.cache),
                    a["setup-timeout"],
                    a["check-timeout"],
                    a["max-configs"],
                    a["sandbox-image"],
                );
                results.push(result);
                writeJSON(file, report);
                console.log(`${repo.repository}: ${result.status}`);
            }
        }
    });
