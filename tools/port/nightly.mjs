// The published package and Cargo metadata identify the same reference everywhere.
export function readPin(cargo) {
  const table = cargo.split("[workspace.metadata.typescript]")[1]?.split(/^\[/m)[0] || "";
  const fields = Object.fromEntries(
    [...table.matchAll(/^(version|commit|nightly)\s*=\s*"([^"]+)"/gm)].map((m) => [m[1], m[2]]),
  );
  if (!fields.version || !fields.nightly || !/^[0-9a-f]{40}$/.test(fields.commit || "")) {
    throw new Error("Invalid Cargo TypeScript metadata");
  }
  return fields;
}

export function nightlyVersion(version) {
  if (!/^\d+\.\d+\.\d+-dev\.\d{8}(?:\.\d+)?$/.test(version || "")) {
    throw new Error("Expected an exact TypeScript nightly version");
  }
  return version;
}

export function packageReference(pkg, version) {
  nightlyVersion(version);
  if (
    pkg.name !== "typescript" ||
    pkg.version !== version ||
    !/^sha512-[A-Za-z0-9+/]+=*$/.test(pkg.dist?.integrity || "") ||
    !/^[0-9a-f]{40}$/.test(pkg.gitHead || "") ||
    pkg.optionalDependencies?.["@typescript/typescript-linux-x64"] !== version ||
    !Object.entries(pkg.optionalDependencies).every(
      ([name, value]) => /^@typescript\/typescript-[a-z0-9-]+$/.test(name) && value === version,
    )
  ) {
    throw new Error("Nightly metadata must identify its exact source and native compiler");
  }
  return { version, sha: pkg.gitHead, integrity: pkg.dist.integrity };
}

export function checkLockfile(lockfile, version, integrities) {
  for (const [name, integrity] of Object.entries(integrities)) {
    const key = `${name}@${version}`.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const match = new RegExp(`^  ['\"]?${key}['\"]?:\\n`, "m").exec(lockfile);
    const block = match ? lockfile.slice(match.index).split(/\n(?=  \S)/)[0] : "";
    const actual = /^    resolution: \{integrity: ([^,\s}]+)\}$/m.exec(block)?.[1];
    if (actual !== integrity)
      throw new Error(`Lockfile integrity does not match published ${name}@${version}`);
  }
}

export function checkStates(checks) {
  if (!checks.length) return "pending";
  if (checks.some((check) => ["fail", "cancel"].includes(check.bucket))) return "failed";
  const required = [
    "Lint",
    "Test Linux",
    "Node API clients and parity",
    "Fourslash",
    "Determinism",
  ];
  return checks.every((check) => ["pass", "skipping"].includes(check.bucket)) &&
    required.every((name) => checks.some((check) => check.name === name && check.bucket === "pass"))
    ? "passed"
    : "pending";
}

export function protectedPath(file) {
  return (
    file === "AGENTS.md" ||
    file === ".github" ||
    file === "tools/port" ||
    file.startsWith(".github/") ||
    file.startsWith("tools/port/") ||
    file === "tools/lib/typescript-reference.mjs" ||
    (file.startsWith("tasks/determinism/") &&
      !/^tasks\/determinism\/snapshots\/[^/]+\.json$/.test(file)) ||
    /(^|\/)README\.md$/.test(file)
  );
}
