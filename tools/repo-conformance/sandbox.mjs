// Run repository-controlled code in a disposable container and export regular files only.
import fs from "node:fs";
import path from "node:path";
import { randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";
import { run, write, writeJSON, json, requireValue } from "../lib/cli.mjs";
export function launch(image, worker, tsgo, tsrs, work, request, log, timeout, source) {
    requireValue(process.getuid() !== 0, "Run repository conformance as a non-root host user");
    const name = `tsrs-conformance-${randomUUID()}`,
        mounts = [
            [path.resolve(path.dirname(worker), ".."), "/harness/tools", true],
            [tsgo, "/inputs/tsgo", true],
            [tsrs, "/inputs/tsrs", true],
            [request, "/request.json", true],
            [work, "/work", false],
        ];
    if (source) mounts.push([source, "/source", true]);
    const command = [
        "docker",
        "run",
        "--pull=never",
        "--name",
        name,
        "--label",
        "tsrs.repo-conformance",
        "--init",
        "--read-only",
        "--user",
        `${process.getuid()}:${process.getgid()}`,
        "--cap-drop=ALL",
        "--security-opt=no-new-privileges",
        "--pids-limit=512",
        "--network=bridge",
        "--workdir=/work",
        "--env=HOME=/work/home",
        "--env=TMPDIR=/work/tmp",
        "--entrypoint=node",
    ];
    for (const [file, target, readonly] of mounts)
        command.push(
            "--mount",
            `type=bind,source=${path.resolve(file)},target=${target}${readonly ? ",readonly" : ""}`,
        );
    command.push(image, "/harness/tools/repo-conformance/run.mjs", "_repo");
    fs.mkdirSync(path.join(work, "tmp"), { recursive: true });
    write(log, "");
    const fd = fs.openSync(log, "a");
    try {
        try {
            const result = run(command, { stdio: ["ignore", fd, fd], check: false, timeout });
            return { exit: result.code, timeout: false };
        } catch (e) {
            if (e.code !== "ETIMEDOUT") throw e;
            return { exit: -1, timeout: true };
        }
    } finally {
        try {
            const result = run(["docker", "rm", "--force", name], {
                stdio: ["ignore", fd, fd],
                timeout: 30,
                check: false,
            });
            requireValue(!result.code, `Could not remove sandbox ${name}; refusing to start another repository`);
        } finally {
            fs.closeSync(fd);
        }
    }
}
export function copyLogs(source, target) {
    if (!fs.existsSync(source) || !fs.lstatSync(source).isDirectory()) return;
    fs.mkdirSync(target, { recursive: true });
    for (const name of fs.readdirSync(source)) {
        const file = path.join(source, name),
            dest = path.join(target, name),
            stat = fs.lstatSync(file);
        if (stat.isDirectory()) copyLogs(file, dest);
        else if (stat.isFile()) fs.copyFileSync(file, dest);
    }
}
export function checkInContainer(
    repo,
    tsgo,
    tsrs,
    out,
    scratch,
    setupTimeout,
    checkTimeout,
    maxConfigs,
    image,
    source,
) {
    fs.mkdirSync(scratch, { recursive: true });
    fs.mkdirSync(out, { recursive: true });
    const base = fs.mkdtempSync(path.join(scratch, "repository-")),
        work = path.join(base, "work"),
        request = path.join(base, "request.json");
    try {
        writeJSON(request, {
            repo,
            setup_timeout: setupTimeout,
            check_timeout: checkTimeout,
            max_configs: maxConfigs,
            source: Boolean(source),
        });
        const result = launch(
            image,
            fileURLToPath(new URL("run.mjs", import.meta.url)),
            tsgo,
            tsrs,
            work,
            request,
            path.join(out, "sandbox.log"),
            setupTimeout + checkTimeout + 30,
            source,
        );
        copyLogs(path.join(work, "out"), out);
        if (result.exit || result.timeout)
            return {
                ...repo,
                status: "setup_failed",
                configs: [],
                reason: "Sandbox failed or timed out; see sandbox.log",
            };
        const file = path.join(work, "result.json"),
            stat = fs.lstatSync(file);
        requireValue(stat.isFile() && stat.size <= 1024 * 1024, "Invalid sandbox result file");
        const data = json(file);
        requireValue(
            data.repository === repo.repository &&
                ["setup_failed", "no_config", "mismatch", "tsrs_failed", "match", "partial", "not_checked"].includes(
                    data.status,
                ),
            "Invalid sandbox repository result",
        );
        return data;
    } finally {
        fs.rmSync(base, { recursive: true, force: true });
    }
}
