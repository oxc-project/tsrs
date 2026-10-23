// Read the existing benchmark recipes and the root pnpm installation for the Rust task.
import fs from "node:fs/promises";
import path from "node:path";
import { createRequire } from "node:module";
import { catalog, fileSha, nativePlatform, overlaySha, projectPath, REPO, selectProjects } from "../../bench/lib.mjs";

const [mode, work, selection] = process.argv.slice(2);
if (mode === "inputs") {
    const cfg = await catalog();
    const inputs = await Promise.all(selectProjects(cfg, selection).map(async (project) => ({
        name: project.name,
        ...projectPath(project, work),
        identity: {
            project,
            suite: project.suite_dir ? cfg.suite : null,
            overlay: await overlaySha(project),
        },
    })));
    process.stdout.write(JSON.stringify(inputs));
} else if (mode === "reference") {
    const rootRequire = createRequire(path.join(REPO, "package.json"));
    const packageFile = rootRequire.resolve("typescript/package.json");
    const pkg = JSON.parse(await fs.readFile(packageFile, "utf8"));
    const native = `@typescript/typescript-${nativePlatform()}`;
    const nativeFile = createRequire(packageFile).resolve(`${native}/package.json`);
    const installed = JSON.parse(await fs.readFile(nativeFile, "utf8"));
    if (pkg.optionalDependencies?.[native] !== installed.version) throw new Error("TypeScript native dependency version mismatch");
    const binary = path.join(path.dirname(nativeFile), "lib/tsc");
    process.stdout.write(JSON.stringify({
        binary,
        provenance: {
            package: pkg.name,
            version: pkg.version,
            native_package: installed.name,
            native_version: installed.version,
            binary_sha256: await fileSha(binary),
        },
    }));
} else {
    throw new Error("Expected inputs or reference");
}
