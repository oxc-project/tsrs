#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { ROOT, args, main, read, required, requireValue, run } from "../lib/cli.mjs";
import { readVersions, validateNextVersion } from "./version.mjs";

main(() => {
    const values = required(
        args("Check whether a push bumped both Cargo and npm release versions.", { before: null }),
        "before",
    );
    requireValue(/^[0-9a-f]{40}$/.test(values.before), "--before must be the full commit SHA before the push");
    const version = (manifest) => {
        const value = manifest.match(/^tsrs_cli\s*=\s*\{[^}]*\bversion\s*=\s*"([^"]+)"/m)?.[1]
            ?? manifest.match(/\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/m)?.[1];
        requireValue(value, "Missing tsrs_cli workspace dependency version in Cargo.toml");
        return value;
    };
    const current = {
        cargo: version(read(path.join(ROOT, "Cargo.toml"))),
        npm: JSON.parse(read(path.join(ROOT, "package.json"))).version,
    };
    const previous = /^0+$/.test(values.before)
        ? { cargo: null, npm: null }
        : {
              cargo: version(run(["git", "show", `${values.before}:Cargo.toml`], { cwd: ROOT }).stdout),
              // Before cargo-release-oxc managed the root version, it included the published TypeScript suffix.
              npm: JSON.parse(run(["git", "show", `${values.before}:package.json`], { cwd: ROOT }).stdout).version?.replace(/-ts(?=\d).*$/, ""),
          };
    requireValue(
        typeof current.npm === "string" && (previous.npm === null || typeof previous.npm === "string"),
        "Missing package.json version",
    );
    const cargoChanged = current.cargo !== previous.cargo;
    const npmChanged = current.npm !== previous.npm;
    requireValue(
        cargoChanged === npmChanged,
        "Bump Cargo and package.json versions together with cargo release-oxc update --release crates --version <version>",
    );
    const changed = cargoChanged && npmChanged;
    requireValue(current.cargo === current.npm, "Cargo and root npm versions must match");
    if (changed && previous.cargo !== null) {
        validateNextVersion(previous.cargo, current.cargo, readVersions(ROOT).typescriptVersion);
    }
    if (process.env.GITHUB_OUTPUT) fs.appendFileSync(process.env.GITHUB_OUTPUT, `changed=${changed}\n`);
    console.log(
        `Cargo ${previous.cargo ?? "(initial push)"} -> ${current.cargo}; npm ${previous.npm ?? "(initial push)"} -> ${current.npm}: ${changed ? "release" : "unchanged"}`,
    );
});
