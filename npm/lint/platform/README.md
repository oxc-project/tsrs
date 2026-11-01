# {{name}}

The `{{platform}}` binary of [`{{mainName}}`](https://www.npmjs.com/package/{{mainName}}), tsrs's type-aware lint
backend for oxlint (tsgolint's headless protocol), with the default TypeScript libraries it reads. Do not install this
package directly: install `{{mainName}}`, which depends on it through `optionalDependencies` and runs it via
`tsrs-lint`.

Version {{version}} ports microsoft/TypeScript@{{commit}}. Licensed under Apache-2.0 as a derivative of
TypeScript; see `LICENSE` and `NOTICE.txt`. The rules are a port of oxc-project/tsgolint, whose MIT license is in
`LICENSE-tsgolint`.
