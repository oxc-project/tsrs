import fs from "node:fs";
import path from "node:path";

// Use numbers npm and Cargo can both represent, and reject SemVer's leading-zero aliases.
export function parseVersion(version) {
    const match = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/.exec(version);
    if (!match) throw new Error(`Invalid release version: ${version}`);
    const core = match.slice(1, 4).map(BigInt);
    const prerelease = match[4]?.split(".") ?? [];
    if (core.some(part => part > BigInt(Number.MAX_SAFE_INTEGER)) || prerelease.some(part =>
        /^0\d+$/.test(part) || (/^\d+$/.test(part) && BigInt(part) > BigInt(Number.MAX_SAFE_INTEGER)),
    )) {
        throw new Error(`Invalid release version: ${version}`);
    }
    return { core, prerelease };
}

export function compareVersions(left, right) {
    const a = parseVersion(left);
    const b = parseVersion(right);
    const compare = (x, y) => x < y ? -1 : x > y ? 1 : 0;
    for (let i = 0; i < 3; i++) {
        const order = compare(a.core[i], b.core[i]);
        if (order) return order;
    }
    if (!a.prerelease.length || !b.prerelease.length) return compare(!a.prerelease.length, !b.prerelease.length);
    for (let i = 0; i < Math.max(a.prerelease.length, b.prerelease.length); i++) {
        const x = a.prerelease[i];
        const y = b.prerelease[i];
        if (x === undefined || y === undefined) return compare(x !== undefined, y !== undefined);
        const numericX = /^\d+$/.test(x);
        const numericY = /^\d+$/.test(y);
        const order = numericX && numericY ? compare(BigInt(x), BigInt(y))
            : numericX !== numericY ? compare(!numericX, !numericY) : compare(x, y);
        if (order) return order;
    }
    return 0;
}

export function distTag(version) {
    return parseVersion(version).prerelease.length ? "next" : "latest";
}

export function validateReleaseVersion(version, typescriptVersion) {
    const release = parseVersion(version);
    const upstream = parseVersion(typescriptVersion);
    const prefix = [...upstream.prerelease];
    // TypeScript's dev label sorts after beta; alpha preserves chronological channel order.
    if (prefix[0] === "dev") prefix[0] = "alpha";
    const revision = /^tsrs([1-9]\d*)$/.exec(release.prerelease.at(-1) ?? "");
    const expected = `${upstream.core.join(".")}-${[...prefix, "tsrs1"].join(".")}`;
    if (release.core.some((part, index) => part !== upstream.core[index])
        || release.prerelease.slice(0, -1).join(".") !== prefix.join(".") || !revision) {
        throw new Error(`${version} must follow ${expected} for TypeScript ${typescriptVersion}, with an unpadded positive tsrs revision`);
    }
    return { counter: BigInt(revision[1]) };
}

export function validateNextVersion(current, requested, typescriptVersion) {
    const next = validateReleaseVersion(requested, typescriptVersion);
    if (compareVersions(requested, current) <= 0) throw new Error(`${requested} must be newer than ${current}`);
    let previous;
    try {
        previous = validateReleaseVersion(current, typescriptVersion);
    } catch {
        // A new upstream version or migration from 0.x starts a fresh revision.
    }
    const expected = previous ? previous.counter + 1n : 1n;
    if (next.counter !== expected) throw new Error(`Expected tsrs counter ${expected} for TypeScript ${typescriptVersion}`);
}

// Reading the pin does not require an aligned version: upstream sync prepares the pin before a release bump.
export function readVersions(root) {
    const cargo = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8");
    const version = cargo.match(/^tsrs_cli\s*=\s*\{[^}]*\bversion\s*=\s*"([^"]+)"/m)?.[1];
    const table = cargo.split("[workspace.metadata.typescript]")[1]?.split(/^\[/m)[0] ?? "";
    const pin = Object.fromEntries([...table.matchAll(/^(version|commit|nightly)\s*=\s*"([^"]+)"/gm)].map(match => [match[1], match[2]]));
    if (!version || !pin.version || !pin.nightly || !/^[0-9a-f]{40}$/.test(pin.commit ?? "")) {
        throw new Error("Cargo.toml needs a versioned tsrs_cli workspace dependency and the TypeScript version/commit/nightly pin");
    }
    return {
        version,
        crateVersion: version,
        typescriptVersion: pin.version,
        typescriptCommit: pin.commit,
        typescriptNightly: pin.nightly,
    };
}
