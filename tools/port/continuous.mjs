#!/usr/bin/env node
// Trusted controller: only the worker executes candidate code; publication consumes data on a fresh runner.
import { execFileSync, spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
  lstatSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  checkStates,
  checkLockfile,
  nightlyVersion,
  packageReference,
  protectedPath,
  readPin,
} from "./nightly.mjs";
import {
  publishedReference,
  verifyReference,
  sourceVersion,
} from "../lib/typescript-reference.mjs";

const root = process.cwd();
const work = join(root, "target/port-agent");
const repository = process.env.GITHUB_REPOSITORY || "oxc-project/tsrs";
const prefix = "port/typescript-nightly/";
const blockerPrefix = "Nightly sync blocked: typescript@";
const opencode = ["dlx", "--package", "@opencode/cli@2.0.32", "--", "opencode"];
const automationRoot = fileURLToPath(new URL("../../", import.meta.url));

function command(program, args, options = {}) {
  return execFileSync(program, args, {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    stdio: ["ignore", "pipe", "pipe"],
    ...options,
  }).trim();
}
function run(program, args, options = {}) {
  const result = spawnSync(program, args, { cwd: root, stdio: "inherit", ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${program} failed (${result.signal || result.status})`);
}
const git = (...args) => command("git", args);
const gh = (...args) => command("gh", [...args, "--repo", repository]);
function output(values) {
  if (!process.env.GITHUB_OUTPUT) return;
  const lines = Object.entries(values)
    .map(([key, value]) => `${key}=${String(value).replaceAll("\n", " ")}`)
    .join("\n");
  writeFileSync(process.env.GITHUB_OUTPUT, `${lines}\n`, { flag: "a" });
}
function blockers() {
  return JSON.parse(
    gh(
      "issue",
      "list",
      "--state",
      "open",
      "--search",
      '"Nightly sync blocked:" in:title',
      "--limit",
      "100",
      "--json",
      "number,title",
    ),
  ).filter((issue) => issue.title.startsWith(blockerPrefix));
}
function load() {
  const m = JSON.parse(process.env.PORT_METADATA || "{}");
  nightlyVersion(m.version);
  if (
    ![m.sha, m.base, m.previous?.commit].every((sha) => /^[0-9a-f]{40}$/.test(sha || "")) ||
    !new RegExp(
      `^${prefix.replaceAll("/", "\\/")}${m.version.replaceAll(".", "\\.")}\\/[0-9]+$`,
    ).test(m.branch) ||
    (m.pr !== null && (!Number.isSafeInteger(m.pr) || m.pr < 1)) ||
    !Array.isArray(m.nativePackages) ||
    !m.nativePackages.every((name) => /^@typescript\/typescript-[a-z0-9-]+$/.test(name)) ||
    typeof m.releaseVersion !== "string" ||
    !/^[0-9A-Za-z.+-]+$/.test(m.releaseVersion) ||
    !m.integrities ||
    Object.keys(m.integrities).length !== m.nativePackages.length + 1 ||
    !["typescript", ...m.nativePackages].every((name) =>
      /^sha512-[A-Za-z0-9+/]+=*$/.test(m.integrities[name] || ""),
    )
  ) {
    throw new Error("Invalid trusted nightly metadata");
  }
  return m;
}
function prepare() {
  const main = git("rev-parse", "HEAD");
  const previous = readPin(readFileSync("Cargo.toml", "utf8"));
  const requested = process.env.RETRY_VERSION?.trim();
  if (requested) nightlyVersion(requested);
  const prs = JSON.parse(
    gh(
      "pr",
      "list",
      "--state",
      "open",
      "--limit",
      "1000",
      "--json",
      "number,headRefName,headRefOid,baseRefName,isCrossRepository,mergeable,url",
    ),
  ).filter(
    (pr) => pr.baseRefName === "main" && !pr.isCrossRepository && pr.headRefName.startsWith(prefix),
  );
  if (prs.length > 1)
    throw new Error("Multiple nightly PRs are open; resolve them before continuing");
  const pr = prs[0];
  const issues = blockers();
  if (issues.length > 1)
    throw new Error("Multiple nightly blockers are open; resolve them before continuing");
  let version;
  let base = main;
  let reason;
  if (pr) {
    const match = /^port\/typescript-nightly\/([^/]+)\/[0-9]+$/.exec(pr.headRefName);
    version = nightlyVersion(match?.[1]);
    const result = spawnSync(
      "gh",
      ["pr", "checks", String(pr.number), "--repo", repository, "--json", "name,bucket"],
      { encoding: "utf8" },
    );
    if (result.error || ![0, 1, 8].includes(result.status) || !result.stdout?.trim())
      throw new Error("Unable to read nightly PR checks");
    const state = checkStates(JSON.parse(result.stdout || "[]"));
    if (pr.mergeable === "CONFLICTING")
      reason = `Resolve merge conflicts on ${pr.url} before retrying`;
    else if (state !== "failed") {
      console.log(
        state === "passed" ? `Waiting for ${pr.url} to merge` : `Waiting for checks on ${pr.url}`,
      );
      output({ status: "waiting", automation_commit: main });
      return;
    } else if (requested !== version)
      reason = `Checks failed on ${pr.url}; dispatch retry_version=${version}`;
    if (requested && requested !== version) throw new Error(`The active nightly is ${version}`);
    if (reason && issues.length) {
      output({ status: "waiting", automation_commit: main });
      return;
    }
    run("git", ["fetch", "origin", `refs/heads/${pr.headRefName}`]);
    base = git("rev-parse", "FETCH_HEAD");
    if (base !== pr.headRefOid) {
      output({ status: "waiting", automation_commit: main });
      return;
    }
  } else {
    if (issues.length && requested !== issues[0].title.slice(blockerPrefix.length)) {
      console.log(`Waiting for blocker #${issues[0].number}`);
      output({ status: "waiting", automation_commit: main });
      return;
    }
    version =
      requested ||
      nightlyVersion(
        JSON.parse(command("vp", ["pm", "view", "typescript@next", "--json"])).version,
      );
  }
  const pkg = JSON.parse(command("vp", ["pm", "view", `typescript@${version}`, "--json"]));
  const selected = packageReference(pkg, version);
  if (!pr && previous.nightly === version) {
    if (previous.commit !== selected.sha)
      throw new Error("Published source metadata changed for the pinned package");
    output({ status: "empty", automation_commit: main });
    return;
  }
  const runId = process.env.GITHUB_RUN_ID;
  if (!/^[0-9]+$/.test(runId || "")) throw new Error("GITHUB_RUN_ID is required");
  const metadata = {
    ...selected,
    releaseVersion: JSON.parse(readFileSync("package.json", "utf8")).version,
    integrities: { typescript: selected.integrity },
    nativePackages: Object.keys(pkg.optionalDependencies),
    previous,
    base,
    branch: pr?.headRefName || `${prefix}${version}/${runId}`,
    pr: pr?.number || null,
    reason,
  };
  for (const name of metadata.nativePackages) {
    const native = JSON.parse(command("vp", ["pm", "view", `${name}@${version}`, "--json"]));
    if (
      native.name !== name ||
      native.version !== version ||
      native.gitHead !== selected.sha ||
      !/^sha512-[A-Za-z0-9+/]+=*$/.test(native.dist?.integrity || "")
    )
      throw new Error(`Invalid native package metadata: ${name}`);
    metadata.integrities[name] = native.dist.integrity;
  }
  output({
    status: reason ? "blocked" : "ready",
    metadata: JSON.stringify(metadata),
    automation_commit: main,
  });
  console.log(`${reason || "Selected nightly"}: typescript@${version} (${selected.sha})`);
}
function checkoutReference(m) {
  if (!existsSync("ts-ref/.git"))
    run("git", [
      "clone",
      "--filter=blob:none",
      "--no-checkout",
      "--single-branch",
      "--branch",
      "main",
      "https://github.com/microsoft/TypeScript.git",
      "ts-ref",
    ]);
  run("git", ["-C", "ts-ref", "fetch", "--filter=blob:none", "origin", m.previous.commit, m.sha]);
  const ancestor = spawnSync("git", [
    "-C",
    "ts-ref",
    "merge-base",
    "--is-ancestor",
    m.previous.commit,
    m.sha,
  ]);
  if (ancestor.status !== 0) {
    const common = command("git", ["-C", "ts-ref", "merge-base", m.previous.commit, m.sha]);
    // A release tag may add only its version constant after branching from main.
    const changes = command("git", [
      "-C",
      "ts-ref",
      "diff",
      "--name-only",
      common,
      m.previous.commit,
      "--",
      "tsc",
      "packages/typescript",
      ":(exclude)tsc/internal/core/version.go",
    ]);
    if (m.previous.nightly.includes("-dev.") || changes)
      throw new Error("Nightly does not include the currently pinned behavior");
  }
  run("git", [
    "-C",
    "ts-ref",
    "sparse-checkout",
    "set",
    "tsc",
    "packages/typescript",
    "tools/scripts/tsc",
  ]);
  run("git", ["-C", "ts-ref", "checkout", "--detach", m.sha]);
  run("git", ["-C", "ts-ref", "diff", "--exit-code", "HEAD", "--", "tsc", "packages/typescript"]);
}
function updatePins(m) {
  const cargo = readFileSync("Cargo.toml", "utf8");
  const next = cargo.replace(
    /(\[workspace\.metadata\.typescript\]\n)([^[]*)/,
    (_, header, table) =>
      header +
      table
        .replace(/^(version|nightly)\s*=\s*"[^"]+"/gm, `$1 = "${m.version}"`)
        .replace(/^commit\s*=\s*"[^"]+"/m, `commit = "${m.sha}"`),
  );
  writeFileSync("Cargo.toml", next);
  for (const file of ["package.json", "npm/package.json"]) {
    const pkg = JSON.parse(readFileSync(file, "utf8"));
    pkg.devDependencies.typescript = m.version;
    if (file === "package.json") {
      pkg.version = m.releaseVersion;
    }
    writeFileSync(file, `${JSON.stringify(pkg, null, 4)}\n`);
  }
  let workspace = readFileSync("pnpm-workspace.yaml", "utf8")
    .replace(/^  - ['"]?(?:@typescript\/typescript-[^@'"\n]+|typescript)@[^'"\n]+['"]?\n/gm, "")
    .replace(
      /^# These two native packages[^\n]*\n/m,
      "# Allow the exact TypeScript reference packages to be installed immediately.\n",
    );
  const exclusions = ["typescript", ...m.nativePackages]
    .map((name) => `  - '${name}@${m.version}'`)
    .join("\n");
  if (!/^minimumReleaseAgeExclude:\s*$/m.test(workspace))
    throw new Error("Missing reference package age exclusions");
  workspace = workspace.replace(
    /^minimumReleaseAgeExclude:\s*$/m,
    `minimumReleaseAgeExclude:\n${exclusions}`,
  );
  writeFileSync("pnpm-workspace.yaml", workspace);
  const catalog = JSON.parse(readFileSync("bench/projects.json", "utf8"));
  catalog.tsgo = { ...catalog.tsgo, package: "typescript", version: m.version };
  catalog.reference = {
    ...catalog.reference,
    package: "typescript",
    version: m.version,
    commit: m.sha,
  };
  writeFileSync("bench/projects.json", JSON.stringify(catalog, null, 2) + "\n");
  const versionFile = "crates/tsrs_core/src/version.rs";
  const version = sourceVersion(root);
  writeFileSync(
    versionFile,
    readFileSync(versionFile, "utf8")
      .replace(/^\/\/ Go defaults to `[^`]+`/m, `// Go defaults to \`${version}\``)
      .replace(/None => "[^"]+"/, `None => "${version}"`),
  );
  run("vp", ["install", "--no-frozen-lockfile", "--ignore-scripts"]);
  checkLockfile(readFileSync("pnpm-lock.yaml", "utf8"), m.version, m.integrities);
}
function configureOpenCode() {
  const account = process.env.CLOUDFLARE_ACCOUNT_ID;
  const gateway = process.env.CLOUDFLARE_GATEWAY_ID;
  const token = process.env.CLOUDFLARE_AI_GATEWAY_TOKEN;
  if (!account || !gateway || !token) {
    throw new Error(
      "CLOUDFLARE_ACCOUNT_ID, CLOUDFLARE_GATEWAY_ID, and CLOUDFLARE_AI_GATEWAY_TOKEN are required",
    );
  }
  if (!/^[a-zA-Z0-9_-]+$/.test(account) || !/^[a-zA-Z0-9_-]+$/.test(gateway)) {
    throw new Error("Invalid Cloudflare account or gateway ID");
  }
  const configDirectory = join(homedir(), ".config", "opencode");
  mkdirSync(configDirectory, { recursive: true });
  const config = {
    $schema: "https://opencode.ai/config.json",
    model: "openai/gpt-6.1-sol",
    update: "disable",
    providers: {
      openai: {
        package: "@opencode/ai/providers/openai/responses",
        settings: {
          baseURL: `https://gateway.ai.cloudflare.com/v1/${account}/${gateway}/openai`,
          apiKey: "",
        },
        headers: { "cf-aig-authorization": "Bearer {env:CLOUDFLARE_AI_GATEWAY_TOKEN}" },
        models: {
          "gpt-6.1-sol": {
            modelID: "gpt-6.1-sol",
            name: "GPT-6.1 Sol",
            capabilities: {
              tools: true,
              input: ["text", "image"],
              output: ["text"],
              reasoning: { supported: true, effort: ["low", "medium", "high", "xhigh", "max"] },
            },
            limit: { context: 1_050_000, output: 128_000 },
          },
        },
      },
    },
  };
  writeFileSync(join(configDirectory, "opencode.json"), `${JSON.stringify(config, null, 2)}\n`, {
    mode: 0o600,
  });
  if (!command("vp", [...opencode, "models", "--standalone"]).includes("openai/gpt-6.1-sol")) {
    throw new Error("OpenCode did not register openai/gpt-6.1-sol from the gateway configuration");
  }
}

function verify(m, reference) {
  if (git("rev-parse", "HEAD") !== m.base) throw new Error("Agent changed Git HEAD");
  const files = git("diff", "--name-only", m.base)
    .split("\n")
    .concat(git("ls-files", "--others", "--exclude-standard").split("\n"));
  if (files.some(protectedPath))
    throw new Error("Agent modified protected automation, instructions or README files");
  const pin = readPin(readFileSync("Cargo.toml", "utf8"));
  if (pin.commit !== m.sha || pin.nightly !== m.version || pin.version !== m.version)
    throw new Error("Agent changed the selected nightly pins");
  for (const file of ["package.json", "npm/package.json"]) {
    if (JSON.parse(readFileSync(file, "utf8")).devDependencies.typescript !== m.version)
      throw new Error(`${file} has the wrong TypeScript dependency`);
  }
  if (JSON.parse(readFileSync("package.json", "utf8")).version !== m.releaseVersion)
    throw new Error("Nightly synchronization must preserve the release request version");
  checkCatalog(
    m,
    (file) => readFileSync(file, "utf8"),
    (file) => git("show", `${m.base}:${file}`),
  );
  checkLockfile(readFileSync("pnpm-lock.yaml", "utf8"), m.version, m.integrities);
  verifyReference(reference, root);
  if (git("-C", "ts-ref", "rev-parse", "HEAD") !== m.sha)
    throw new Error("Reference checkout changed");
  run("git", ["-C", "ts-ref", "diff", "--exit-code", "HEAD", "--", "tsc", "packages/typescript"]);
  const env = { ...process.env, TSRS_TS_VERSION: sourceVersion(root) };
  delete env.CLOUDFLARE_AI_GATEWAY_TOKEN;
  run("git", ["diff", "--check"]);
  run("vp", ["install", "--frozen-lockfile", "--ignore-scripts"], { env });
  run("node", ["npm/sdk/sync-upstream.mjs", "--check"], { env });
  run("node", [join(automationRoot, "tools/port/generated.mjs"), "--check"], { env });
  run("cargo", ["fmt", "--all", "--", "--check"], { env });
  run(
    "cargo",
    [
      "build",
      "--profile",
      "dev",
      "--locked",
      "-p",
      "tsrs_cli",
      "-p",
      "tsrs_testrunner",
      "-p",
      "tsrs_determinism",
    ],
    { env },
  );
  run("cargo", ["test", "--locked"], { env });
  run("cargo", ["clippy", "--all-targets", "--locked", "--config", 'build.warnings="deny"'], {
    env,
  });
  run("vp", ["run", "lint:source"], { env });
  run("vp", ["run", "check:scripts"], { env });
  // Freshness compares generated files with the index, including this uncommitted port.
  run("git", ["add", "--all"]);
  run("tools/gen-check.sh", [], { env });
  run(
    "target/debug/tsrs_determinism",
    ["conformance", "--timeout", "900", "--out-dir", join(work, "test-results")],
    { env },
  );
  run("tools/regressions.sh", ["target/debug/tsrs"], { env });
  run("node", [join(automationRoot, "tools/port/build-published.mjs")], { env });
  run("cargo", ["check", "--locked", "-p", "tsrs_wasm", "--target", "wasm32-wasip1"], {
    env: { ...env, TSRS_TS_VERSION: m.version },
  });
  const candidate = join(root, "target/published/debug/tsrs");
  run("node", [join(automationRoot, "tools/port/version-check.mjs"), candidate], { env });
  run("tools/node-api/setup.sh", ["--published-reference"], { env });
  for (const [label, binary] of [
    ["go-oracle", "tools/node-api/.work/published-oracle/tsc"],
    ["tsrs", candidate],
  ])
    run(
      "node",
      [
        "tools/node-api/run-upstream.mjs",
        "--binary",
        binary,
        "--label",
        label,
        "--suite",
        "all",
        "--no-trace",
      ],
      { env },
    );
  run(
    "node",
    [
      "npm/sdk/method-coverage.mjs",
      "--require-pass",
      "--server",
      `tsgo=${reference.binary}`,
      "--server",
      `tsrs=${candidate}`,
    ],
    { env },
  );
  const host = command("rustc", ["-vV"]).match(/^host: (.+)$/m)?.[1];
  run("node", ["npm/build.mjs", "--binary", `${host}=${candidate}`, "--pack"], { env });
  run("node", ["npm/sdk/smoke-consumer.mjs", "--noembed"], { env });
  // The oracle is always the newly pinned npm compiler, never the candidate Rust binary.
  run("target/debug/tsrs_determinism", ["snapshot"], { env });
  run("target/debug/tsrs_determinism", ["check", "--tsrs", candidate], { env });
  run("target/debug/tsrs_determinism", ["regressions", "--tsrs", candidate], { env });
  verifyReference(reference, root);
  const report = readFileSync(join(work, "report.md"), "utf8").trim();
  if (!report || report.length > 16_000) throw new Error("Missing or oversized nightly report");
  if (git("rev-parse", "HEAD") !== m.base) throw new Error("Validation changed Git HEAD");
  run("git", ["add", "--all"]);
  const staged = git("diff", "--cached", "--name-only", m.base).split("\n");
  if (staged.some(protectedPath)) throw new Error("Validation modified protected files");
  run("git", ["diff", "--cached", "--check"]);
  writeFileSync(
    join(work, "port.patch"),
    execFileSync("git", ["diff", "--cached", "--binary", m.base], { maxBuffer: 64 * 1024 * 1024 }),
  );
}
function agent() {
  const m = load();
  mkdirSync(work, { recursive: true });
  rmSync(join(work, "port.patch"), { force: true });
  try {
    checkoutReference(m);
    configureOpenCode();
    const prompt = readFileSync(join(automationRoot, "tools/port/agent-prompt.md"), "utf8")
      .replaceAll("{VERSION}", m.version)
      .replaceAll("{SHA}", m.sha)
      .replaceAll("{PREVIOUS}", m.previous.nightly)
      .replaceAll("{FROM_SHA}", m.previous.commit);
    for (let attempt = 1; attempt <= 2; attempt++) {
      run("git", ["reset", "--hard", m.base]);
      run("git", ["clean", "-fd"]);
      run("git", ["-C", "ts-ref", "reset", "--hard", m.sha]);
      run("git", ["-C", "ts-ref", "clean", "-fd"]);
      rmSync(join(work, "report.md"), { force: true });
      try {
        updatePins(m);
        const reference = publishedReference(root);
        const selectedPrompt = prompt.replaceAll("{REFERENCE_BINARY}", reference.binary);
        run(
          "vp",
          [
            ...opencode,
            "run",
            "--standalone",
            "--auto",
            "--model",
            "openai/gpt-6.1-sol",
            selectedPrompt,
          ],
          { timeout: 130 * 60 * 1000 },
        );
        verify(m, reference);
        return;
      } catch (error) {
        if (attempt === 2) throw error;
        console.error(`Nightly attempt ${attempt} failed: ${error.message}`);
      }
    }
  } catch (error) {
    rmSync(join(work, "port.patch"), { force: true });
    writeFileSync(join(work, "failure.txt"), String(error.message).slice(0, 16_000));
    throw error;
  }
}
function artifact(name, limit = 16_000) {
  const file = join(resolve(process.env.PORT_ARTIFACT || "artifact"), name);
  const stat = lstatSync(file);
  if (!stat.isFile() || stat.size > limit) throw new Error(`Invalid ${name} artifact`);
  return readFileSync(file, "utf8");
}
function indexPin(m) {
  const pin = readPin(git("show", ":Cargo.toml"));
  if (pin.commit !== m.sha || pin.nightly !== m.version || pin.version !== m.version)
    throw new Error("Patch does not select the trusted nightly");
  for (const file of ["package.json", "npm/package.json"]) {
    if (JSON.parse(git("show", `:${file}`)).devDependencies.typescript !== m.version)
      throw new Error("Patch has inconsistent npm pins");
  }
  if (JSON.parse(git("show", ":package.json")).version !== m.releaseVersion)
    throw new Error("Patch requests an npm release");
  checkCatalog(
    m,
    (file) => git("show", `:${file}`),
    (file) => git("show", `HEAD:${file}`),
  );
  checkLockfile(git("show", ":pnpm-lock.yaml"), m.version, m.integrities);
  if (JSON.parse(git("show", ":npm/tsrs/UPSTREAM.json")).commit !== m.sha)
    throw new Error("SDK manifest has the wrong source");
  const projects = JSON.parse(
    readFileSync(join(automationRoot, "bench/projects.json"), "utf8"),
  ).projects;
  for (const project of projects) {
    const snapshot = JSON.parse(git("show", `:tasks/determinism/snapshots/${project.name}.json`));
    if (
      snapshot.typescript_commit !== m.sha ||
      snapshot.reference?.version !== m.version ||
      snapshot.reference?.native_version !== m.version
    )
      throw new Error(`Stale snapshot for ${project.name}`);
  }
}
function checkCatalog(m, read, base) {
  const catalog = JSON.parse(read("bench/projects.json")),
    previous = JSON.parse(base("bench/projects.json"));
  if (
    catalog.reference?.package !== "typescript" ||
    catalog.reference.version !== m.version ||
    catalog.reference.commit !== m.sha ||
    catalog.tsgo?.package !== "typescript" ||
    catalog.tsgo.version !== m.version
  )
    throw new Error("Benchmark references do not match the nightly");
  for (const key of ["suite", "projects"])
    if (JSON.stringify(catalog[key]) !== JSON.stringify(previous[key]))
      throw new Error(`Nightly sync changed benchmark ${key}`);
}
function publish() {
  const m = load();
  if (git("rev-parse", "HEAD") !== m.base) throw new Error("Unexpected publication base");
  if (m.pr) {
    const pr = JSON.parse(gh("pr", "view", String(m.pr), "--json", "state,headRefName,headRefOid"));
    if (pr.state !== "OPEN" || pr.headRefName !== m.branch || pr.headRefOid !== m.base)
      throw new Error("Nightly PR changed while the worker ran");
  }
  const report = artifact("report.md");
  if (!report.trim()) throw new Error("Empty worker report");
  const patch = artifact("port.patch", 64 * 1024 * 1024);
  // Apply only to the index: no candidate program, filter, workflow or hook is executed.
  command("git", ["apply", "--cached", "--binary", "--whitespace=error-all", "-"], {
    input: patch,
    stdio: ["pipe", "pipe", "pipe"],
  });
  const files = git("diff", "--cached", "--name-only", "-z").split("\0").filter(Boolean);
  if (!files.length || files.some(protectedPath))
    throw new Error("Patch changes protected files or is empty");
  indexPin(m);
  run("git", ["diff", "--cached", "--check"]);
  const id = command("gh", ["api", "users/oxc-guard[bot]", "--jq", ".id"]);
  run("git", ["config", "user.name", "oxc-guard[bot]"]);
  run("git", ["config", "user.email", `${id}+oxc-guard[bot]@users.noreply.github.com`]);
  run("gh", ["auth", "setup-git"]);
  run("git", [
    "-c",
    "core.hooksPath=/dev/null",
    "commit",
    "-m",
    `chore(port): sync TypeScript ${m.version}`,
    "-m",
    `Upstream-Commit: ${m.sha}\nTypeScript-Nightly: ${m.version}`,
  ]);
  run("git", ["-c", "core.hooksPath=/dev/null", "push", "origin", `HEAD:refs/heads/${m.branch}`]);
  if (!m.pr) {
    const body = join(mkdtempSync(join(tmpdir(), "nightly-pr-")), "body.md");
    writeFileSync(
      body,
      `Syncs TypeScript ${m.version} (${m.sha}). Validation report: https://github.com/${repository}/actions/runs/${process.env.GITHUB_RUN_ID}\n`,
    );
    gh(
      "pr",
      "create",
      "--draft",
      "--base",
      "main",
      "--head",
      m.branch,
      "--title",
      `chore(port): sync TypeScript ${m.version}`,
      "--body-file",
      body,
    );
  }
  for (const issue of blockers().filter((issue) => issue.title === blockerPrefix + m.version))
    gh("issue", "close", String(issue.number));
}
function block() {
  const m = load();
  if (blockers().some((issue) => issue.title === blockerPrefix + m.version))
    throw new Error(`Nightly ${m.version} remains blocked`);
  let reason =
    m.reason || process.env.PORT_BLOCK_REASON || "The nightly worker or publication failed.";
  if (process.env.PORT_ARTIFACT && existsSync(join(process.env.PORT_ARTIFACT, "failure.txt"))) {
    try {
      reason = artifact("failure.txt");
    } catch {
      // A malformed worker artifact must not prevent trusted blocker reporting.
    }
  }
  const body = join(mkdtempSync(join(tmpdir(), "nightly-blocker-")), "body.md");
  writeFileSync(
    body,
    `Sync of typescript@${m.version} (${m.sha}) stopped.\n\n${reason.slice(0, 16_000)}\n\nRun: https://github.com/${repository}/actions/runs/${process.env.GITHUB_RUN_ID}\n\nAfter intervention, dispatch **Sync TypeScript nightlies** with retry_version=\`${m.version}\`. A published PR with failing checks requires manual dispatch to repair it.\n`,
  );
  gh("issue", "create", "--title", blockerPrefix + m.version, "--body-file", body);
  throw new Error(`Nightly ${m.version} is blocked`);
}
try {
  if (repository !== "oxc-project/tsrs") throw new Error(`Unexpected repository: ${repository}`);
  switch (process.argv[2]) {
    case "prepare":
      prepare();
      break;
    case "agent":
      agent();
      break;
    case "publish":
      publish();
      break;
    case "block":
      block();
      break;
    default:
      throw new Error("Usage: continuous.mjs prepare|agent|publish|block");
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
