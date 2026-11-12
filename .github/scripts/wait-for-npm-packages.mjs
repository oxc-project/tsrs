#!/usr/bin/env node
// Wait for optional dependencies, or --release <manifests...> for every package and its release tag.

import fs from "node:fs";
import { distTag } from "../../tools/release/version.mjs";

const sleep = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

export async function checkPackage([name, version], verifyTag = false, fetchPackage = fetch) {
    const metadataUrl = `https://registry.npmjs.org/${encodeURIComponent(name)}/${encodeURIComponent(version)}`;
    const options = { headers: { "cache-control": "no-cache" }, signal: AbortSignal.timeout(30_000) };
    const metadata = await fetchPackage(metadataUrl, options);
    if (!metadata.ok) throw new Error(`metadata unavailable for ${name}@${version}`);
    const data = await metadata.json();
    if (data.name !== name || data.version !== version) throw new Error(`unexpected metadata for ${name}@${version}`);
    const tarball = await fetchPackage(data.dist.tarball, {
        headers: { "cache-control": "no-cache", range: "bytes=0-0" },
        signal: AbortSignal.timeout(30_000),
    });
    await tarball.body?.cancel();
    if (!tarball.ok) throw new Error(`tarball unavailable for ${name}@${version}`);
    if (verifyTag) {
        const channel = distTag(version);
        const tag = await fetchPackage(`https://registry.npmjs.org/${encodeURIComponent(name)}/${channel}`, options);
        if (!tag.ok || (await tag.json()).version !== version) throw new Error(`${channel} does not name ${name}@${version}`);
    }
}

async function waitForPackage([name, version], verifyTag) {
    const spec = `${name}@${version}`;
    // Allow slow registry staging, with room for both waits within the release job's 90-minute timeout.
    const deadline = Date.now() + 30 * 60 * 1000;

    console.log(`Waiting for ${spec} to be available.`);
    while (Date.now() < deadline) {
        try {
            await checkPackage([name, version], verifyTag);
            console.log(`✓ ${spec} is available.`);
            return;
        } catch {
            await sleep(10_000);
        }
    }

    throw new Error(`Timed out waiting for ${spec}.`);
}

export function releasePackages(manifests) {
    const version = manifests[0]?.version;
    if (!version) throw new Error("release manifests must have a version");
    return manifests.flatMap(manifest => {
        if (manifest.version !== version) throw new Error("release manifest versions differ");
        if (!manifest.packages?.length) throw new Error("release manifest has no packages");
        return manifest.packages.map(pkg => {
            if (!pkg.name) throw new Error("release package has no name");
            return [pkg.name, version];
        });
    });
}

if (import.meta.main) {
    try {
        const verifyTag = process.argv[2] === "--release";
        const packages = verifyTag
            ? releasePackages(process.argv.slice(3).map(file => JSON.parse(fs.readFileSync(file, "utf8"))))
            : Object.entries(JSON.parse(fs.readFileSync(process.argv[2], "utf8")).optionalDependencies);
        await Promise.all(packages.map(pkg => waitForPackage(pkg, verifyTag)));
    } catch (error) {
        console.error(`::error::${error.message}`);
        process.exitCode = 1;
    }
}
