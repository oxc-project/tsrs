#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { ROOT, args, read, run, walk, requireValue, main, isMain } from "../../lib/cli.mjs";
import { unified } from "../../lib/diff.mjs";
export function referenceBinary() {
    const exe = process.env.TSGO ?? (process.env.TSRS_WORK ? path.join(process.env.TSRS_WORK, "bin/tsgo-ref") : null);
    requireValue(exe, "set TSGO to the reference tsgo binary");
    return exe;
}
export const tsrsBinary = () => process.env.TSRS ?? path.join(ROOT, "target/release/tsrs");
function canonical(file) {
    let parent = file,
        suffix = [];
    while (!fs.existsSync(parent)) {
        suffix.unshift(path.basename(parent));
        const next = path.dirname(parent);
        if (next === parent) break;
        parent = next;
    }
    return path.join(fs.realpathSync(parent), ...suffix);
}
export const overlaps = (a, b) => {
    a = canonical(a);
    b = canonical(b);
    return a === b || a.startsWith(b + path.sep) || b.startsWith(a + path.sep);
};
if (isMain(import.meta.url))
    main(() => {
        const raw = process.argv.slice(2),
            sep = raw.indexOf("--"),
            extra = sep < 0 ? [] : raw.slice(sep + 1),
            a = args(
                "Emit a project with both compilers and compare all outputs. <project> [-- compiler flags]",
                {
                    name: null,
                    "go-out": null,
                    "rs-out": null,
                    json: false,
                    timeout: 900,
                    quiet: false,
                    buildinfo: false,
                },
                sep < 0 ? raw : raw.slice(0, sep),
            );
        requireValue(a._.length === 1, "one project is required");
        const project = path.resolve(a._[0]),
            name = a.name ?? path.basename(project),
            work = path.join(ROOT, "target/scratch/emit-oracle", name),
            goOut = path.resolve(a["go-out"] ?? path.join(work, "ref/out")),
            rsOut = path.resolve(a["rs-out"] ?? path.join(work, "rs/out"));
        if ([goOut, goOut + ".tsbuildinfo"].some((g) => [rsOut, rsOut + ".tsbuildinfo"].some((r) => overlaps(g, r)))) {
            console.error("--go-out and --rs-out overlap; the reference and tsrs outputs must be disjoint");
            process.exitCode = 2;
            return;
        }
        const layout = (out) => [path.basename(out), path.relative(path.dirname(out), project)];
        if (a.buildinfo && JSON.stringify(layout(goOut)) !== JSON.stringify(layout(rsOut))) {
            console.error("--buildinfo needs output directories with the same basename and path to the project");
            process.exitCode = 2;
            return;
        }
        // Reject project/output overlap before deleting or launching a compiler.
        requireValue(
            ![goOut, rsOut].some((out) => overlaps(out, project)),
            "output directories must be outside the project",
        );
        for (const out of [goOut, rsOut]) {
            fs.rmSync(out, { recursive: true, force: true });
            fs.rmSync(out + ".tsbuildinfo", { force: true });
        }
        const cwd = fs.statSync(project).isDirectory() ? project : path.dirname(project),
            flags = (out) => [
                "-p",
                project,
                "--outDir",
                out,
                "--declarationDir",
                out,
                "--tsBuildInfoFile",
                out + ".tsbuildinfo",
                "--pretty",
                "false",
                ...extra,
            ];
        const compile = (binary, out, env) => {
            try {
                const r = run([binary, ...flags(out)], { cwd, env, timeout: a.timeout, check: false });
                return [r.code, r.stdout + r.stderr];
            } catch (e) {
                if (e.code === "ETIMEDOUT") return ["timeout", ""];
                throw e;
            }
        };
        const ref = referenceBinary(),
            env = { ...process.env },
            refDir = path.dirname(fs.realpathSync(ref));
        if (a.buildinfo && !env.TSRS_LIB_PATH && fs.existsSync(path.join(refDir, "lib.d.ts")))
            env.TSRS_LIB_PATH = refDir;
        const [goStatus, goText] = compile(ref, goOut, process.env),
            [rsStatus, rsText] = compile(tsrsBinary(), rsOut, env),
            gText = goText.replaceAll(goOut, "<OUT>"),
            rText = rsText.replaceAll(rsOut, "<OUT>");
        const tree = (out) => new Map(fs.existsSync(out) ? [...walk(out)].map((f) => [path.relative(out, f), f]) : []),
            go = tree(goOut),
            rs = tree(rsOut);
        if (a.buildinfo)
            for (const [out, map] of [
                [goOut, go],
                [rsOut, rs],
            ])
                if (fs.existsSync(out + ".tsbuildinfo"))
                    map.set(path.basename(out) + ".tsbuildinfo", out + ".tsbuildinfo");
        const result = {
                name,
                identical: 0,
                different: 0,
                missing: 0,
                extra: 0,
                ref_status: goStatus,
                rs_status: rsStatus,
                diagnostics_match: gText === rText,
                rs_panic: "",
            },
            details = [];
        for (const [rel, file] of go) {
            if (!rs.has(rel)) {
                result.missing++;
                details.push(`missing: ${rel}`);
            } else if (fs.readFileSync(file).equals(fs.readFileSync(rs.get(rel)))) result.identical++;
            else {
                result.different++;
                if (details.length < 5)
                    details.push(
                        `--- ${rel}\n${unified(read(file), read(rs.get(rel)), `ref/${rel}`, `rs/${rel}`)
                            .split("\n")
                            .slice(0, 30)
                            .join("\n")}`,
                    );
            }
        }
        for (const rel of rs.keys())
            if (!go.has(rel)) {
                result.extra++;
                details.push(`extra: ${rel}`);
            }
        if (rsText.includes("panicked at") || ![0, 1, 2].includes(rsStatus))
            result.rs_panic =
                rsText
                    .split("\n")
                    .find((l) => l.includes("not ported"))
                    ?.trim() ??
                rsText
                    .split("\n")
                    .find((l) => l.includes("panicked at"))
                    ?.trim() ??
                "";
        const ok =
            !result.different &&
            !result.missing &&
            !result.extra &&
            goStatus === rsStatus &&
            result.diagnostics_match &&
            [0, 1, 2, 3, 4].includes(goStatus) &&
            !result.rs_panic;
        if (a.json) console.log(JSON.stringify(result));
        else {
            console.log(
                `files: ${result.identical} identical, ${result.different} different, ${result.missing} missing, ${result.extra} extra`,
            );
            console.log(
                `exit: ref ${goStatus}, tsrs ${rsStatus}; diagnostics ${result.diagnostics_match ? "match" : "differ"}`,
            );
            if (result.rs_panic) console.log(`tsrs panic: ${result.rs_panic}`);
            if (!a.quiet) {
                for (const d of details.slice(0, 20)) console.log(d);
                if (!result.diagnostics_match) console.log(unified(gText, rText, "ref", "tsrs").slice(0, 3000));
            }
        }
        process.exitCode = ok ? 0 : 1;
    });
