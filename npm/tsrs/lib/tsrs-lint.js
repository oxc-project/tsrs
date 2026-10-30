// `tsrs-lint`: the type-aware linter, `tsrs --lint`. oxlint runs it as `<path> headless ...`
// (OXLINT_TSGOLINT_PATH=node_modules/.bin/tsrs-lint).
process.argv.splice(2, 0, "--lint");
await import("./tsrs.js");
