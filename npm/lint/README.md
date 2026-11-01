# @tsrs/lint

Type-aware linting for [oxlint](https://oxc.rs/docs/guide/usage/linter/type-aware) on the tsrs type checker.
`tsrs-lint` speaks tsgolint's headless protocol and implements tsgolint's rules (`crates/tsrs_lint` in
[oxc-project/tsrs](https://github.com/oxc-project/tsrs)), so oxlint can run it in place of `oxlint-tsgolint`.

```sh
npm add -D @tsrs/lint
OXLINT_TSGOLINT_PATH=node_modules/.bin/tsrs-lint oxlint --type-aware
```

oxlint looks for `tsgolint` unless `OXLINT_TSGOLINT_PATH` names another executable. Set it wherever lint runs (CI,
editor, hooks).

The binary comes from one optional dependency per platform, `@tsrs/lint-<os>-<cpu>[-<libc>]`, which also holds the
default TypeScript libraries. Releases share tsrs's version. `TSRS_LINT_BINARY=/path/to/tsrs-lint` runs a locally
built binary instead.

Licensed under Apache-2.0 as a derivative of TypeScript (see `LICENSE` and `NOTICE.txt`); the rules are a port of
oxc-project/tsgolint, whose MIT license is in `LICENSE-tsgolint`.
