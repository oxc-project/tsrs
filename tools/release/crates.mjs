#!/usr/bin/env node
// Package the release together so Cargo can verify dependencies before their first crates.io publication.
import path from "node:path";
import { ROOT, args, main, read, requireValue, run } from "../lib/cli.mjs";

main(() => {
    const values = args("Validate and package the publishable Rust crates.", {
        check: false,
        verify: true,
        "allow-dirty": false,
    });
    run([process.execPath, "tools/gen-release-version.mjs", "--check"], { cwd: ROOT });
    const metadata = JSON.parse(
        run(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], { cwd: ROOT }).stdout,
    );
    const members = metadata.packages.filter((pkg) => metadata.workspace_members.includes(pkg.id));
    const version = members.find((pkg) => pkg.name === "tsrs_cli")?.version;
    const packages = members.filter((pkg) => pkg.publish === null);
    const privateCrates = new Set(["tsrs_bench", "tsrs_contentmappertest", "tsrs_testrunner", "tsrs_wasm"]);
    const unexpected = packages.filter((pkg) =>
        privateCrates.has(pkg.name) || pkg.manifest_path.startsWith(path.join(ROOT, "tasks") + path.sep),
    );
    requireValue(!unexpected.length, `Excluded crates must stay unpublished: ${unexpected.map((pkg) => pkg.name).join(", ")}`);
    requireValue(packages.length && version, "No publishable workspace crates or shared version found");
    const npmVersion = JSON.parse(read(path.join(ROOT, "package.json"))).version;
    const expectedNpmVersion = run([process.execPath, "npm/build.mjs", "--print-crate-version"], { cwd: ROOT }).stdout.trim();
    requireValue(
        expectedNpmVersion === version,
        `tsrs_cli workspace dependency version ${expectedNpmVersion} must match its package version ${version}`,
    );
    requireValue(
        npmVersion === expectedNpmVersion,
        `package.json version ${npmVersion} must match ${expectedNpmVersion} from Cargo.toml`,
    );
    const names = new Set(packages.map((pkg) => pkg.name));
    for (const pkg of packages) {
        requireValue(pkg.version === version, `${pkg.name}: version must match tsrs_cli version ${version}`);
        requireValue(pkg.description && pkg.license && pkg.repository && pkg.readme, `${pkg.name}: incomplete release metadata`);
        for (const dep of pkg.dependencies.filter((dep) => dep.path && dep.kind !== "dev")) {
            requireValue(names.has(dep.name), `${pkg.name}: runtime dependency ${dep.name} is not publishable`);
            requireValue(dep.req === `^${version}`, `${pkg.name}: ${dep.name} must require version ${version}`);
        }
    }
    console.log(`Release ${version}: ${packages.length} crates (${packages.map((pkg) => pkg.name).join(", ")})`);
    if (values.check) return;
    run(
        [
            "cargo", "package", "--locked",
            ...packages.flatMap((pkg) => ["-p", pkg.name]),
            ...(!values.verify ? ["--no-verify"] : []),
            ...(values["allow-dirty"] ? ["--allow-dirty"] : []),
        ],
        { cwd: ROOT, inherit: true, timeout: 3600 },
    );
});
