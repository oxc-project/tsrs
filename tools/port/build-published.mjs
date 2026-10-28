#!/usr/bin/env node
// Match npm's version and on-disk library layout without replacing the source-baseline build.
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { referencePin } from "../lib/typescript-reference.mjs";
const profile = process.argv[2] || "dev";
if (!["dev", "ci"].includes(profile)) throw new Error("Usage: build-published.mjs [dev|ci]");
const pin = referencePin(process.cwd());
execFileSync(
  "cargo",
  ["build", "--profile", profile, "--locked", "-p", "tsrs_cli", "--features", "noembed"],
  {
    stdio: "inherit",
    env: {
      ...process.env,
      TSRS_TS_VERSION: pin.nightly,
      CARGO_TARGET_DIR: path.resolve("target/published"),
    },
  },
);
const directory = path.resolve("target/published", profile === "dev" ? "debug" : profile);
for (const file of fs.readdirSync(directory))
  if (/^lib.*\.d\.ts$/.test(file)) fs.rmSync(path.join(directory, file));
const libraries = path.resolve("crates/tsrs_vfs/libs");
for (const file of fs.readdirSync(libraries))
  if (/^lib.*\.d\.ts$/.test(file))
    fs.copyFileSync(path.join(libraries, file), path.join(directory, file));
