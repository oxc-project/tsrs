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
    const [major, minor, patch] = upstream.core;
    const base = patch * 1000n;
    if (release.core[0] !== major || release.core[1] !== minor) {
        throw new Error(`${version} must use TypeScript ${typescriptVersion}'s major and minor versions`);
    }
    const counter = release.core[2] - base;
    if (counter < 1n || counter > 999n) {
        throw new Error(`${version} must use TypeScript patch ${patch} × 1000 + a suffix from 001 to 999; upgrade TypeScript instead of rolling the suffix over`);
    }
    if (release.prerelease.join(".") !== upstream.prerelease.join(".")) {
        throw new Error(`${version} must preserve TypeScript ${typescriptVersion}'s prerelease identifiers`);
    }
    return { counter };
}

export function validateNextVersion(current, requested, typescriptVersion) {
    const next = validateReleaseVersion(requested, typescriptVersion);
    if (compareVersions(requested, current) <= 0) throw new Error(`${requested} must be newer than ${current}`);
    const previous = parseVersion(current);
    const upstream = parseVersion(typescriptVersion);
    const counter = previous.core[2] - upstream.core[2] * 1000n;
    // Keep counting across prereleases and stable promotion of the same TypeScript numeric version.
    const sameReference = previous.core[0] === upstream.core[0] && previous.core[1] === upstream.core[1]
        && counter >= 1n && counter <= 999n;
    const expected = sameReference ? counter + 1n : 1n;
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
