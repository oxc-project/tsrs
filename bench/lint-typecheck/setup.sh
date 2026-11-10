#!/bin/bash
# Prepares a Linux machine (Ubuntu 24.04, user ubuntu, Node 24, cgroup v2) for measure.mjs: pinned tools in ~/tools,
# pinned projects in ~/proj with dependencies installed without install scripts.
set -euxo pipefail
mkdir -p ~/tools/tsrs ~/tools/tsgolint ~/tools/eslint ~/proj ~/lint-typecheck
install() { (cd ~/tools/$1 && { [ -f package.json ] || npm init -y > /dev/null; } && shift && npm i -E "$@"); }
install tsrs tsrs@0.24.0-ts7.1.0-beta oxlint@1.93.0
install tsgolint oxlint@1.93.0 oxlint-tsgolint@7.0.2006 typescript@7.0.2
install eslint eslint@10.14.0 typescript-eslint@8.72.2 typescript@6.0.3

cd ~/proj
[ -d vscode ] || git clone -q --depth 1 --branch 1.145.2 https://github.com/microsoft/vscode
[ -d outline ] || git clone -q --depth 1 --branch v1.12.0 https://github.com/outline/outline
[ -d vue ] || git clone -q --depth 1 --branch v3.5.45 https://github.com/vuejs/core vue

(cd vscode && npm ci --ignore-scripts --no-audit --no-fund)
# --ignore-scripts skips VS Code's postinstall, which downloads the Electron typings.
(cd vscode && node build/npm/electronTypes.ts)
(cd outline && YARN_ENABLE_SCRIPTS=0 corepack yarn install --immutable --mode=skip-build)
# Only the rule set under test runs.
find outline -name .oxlintrc.json -not -path '*/node_modules/*' -delete
(cd vue && corepack pnpm install --frozen-lockfile --ignore-scripts)
# A composite project rejects --incremental false; isolatedDeclarations needs composite or declaration.
node -e '
const fs = require("fs");
const config = JSON.parse(fs.readFileSync("vue/tsconfig.json", "utf8"));
config.compilerOptions.composite = false;
delete config.compilerOptions.isolatedDeclarations;
fs.writeFileSync("vue/tsconfig.json", JSON.stringify(config, null, 2) + "\n");
'
echo SETUP DONE
