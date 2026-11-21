# The lint server (`tsrs-lint server`)

`tsrs-lint server` is a long-lived, type-aware linter for oxlint's language server. Headless mode
(`tsrs-lint headless`) builds every program from scratch for each run. The server keeps a project session instead
(the one `tsrs --lsp` uses): it holds the editor's unsaved text of open files, reuses programs across edits and keeps
checkers warm, so a lint request after an edit only pays for what changed. The client starts one server per
workspace, in the workspace directory, and talks to it over stdin and stdout.

```sh
tsrs-lint server [--runExternalCode]
```

`--runExternalCode` (or `TSRS_LINT_RUN_EXTERNAL_CODE=1`) runs content mappers, as in headless mode. Fix flags come
with each lint request. The implementation is `crates/tsrs_lint/src/server.rs`; the tests that drive the binary as a
client would are in `crates/tsrs_lint/tests/integration/server.rs`.

## Frames

Both directions use headless mode's framing: a little-endian `u32` byte length of the JSON, a `u8` message type,
then the UTF-8 JSON, which is always an object. Paths should be absolute; backslashes are accepted.

The client sends:

| type | name | JSON |
| --- | --- | --- |
| 16 | `DidOpen` | `{"path", "version", "text"}` |
| 17 | `DidChange` | `{"path", "version", "text"}` (the whole text) |
| 18 | `DidClose` | `{"path"}` |
| 19 | `FilesChanged` | `{"changes": [{"path", "kind"}]}` (LSP `FileChangeType`: 1 created, 2 changed, 3 deleted) |
| 20 | `Lint` | `{"id", "configs": [{"file_paths", "rules"}], "fix", "fix_suggestions"}` |
| 21 | `Shutdown` | `{}` |

The server sends:

| type | name | JSON |
| --- | --- | --- |
| 0 | `Error` | `{"id", "error"}` |
| 1 | `Diagnostic` | headless mode's diagnostic object, with the request's `"id"` as its first field |
| 3 | `Done` | `{"id"}` |

`id` is a non-negative integer the client chooses; ids should be unique among the requests in flight. A `Lint`
request's `configs` and `rules` are headless mode's (V2 payload); each file is linted once, with the rules of the
first config that names it. Compiler diagnostics are not reported. Tsconfig and program errors of a configured
project are reported as in headless mode, on the tsconfig, once per request, and that project's files are not
linted.

## Errors

- A frame that cannot be read, a frame of an unknown message type, a malformed notification (`DidOpen`,
  `DidChange`, `DidClose`, `FilesChanged`, `Shutdown`), or a `Lint` frame without a valid `id` is a fatal error: an
  error frame with a null `id`, after which the server exits with status 1. Going on would let the client's view of
  open files and the server's diverge.
- A `Lint` request that cannot be parsed otherwise (an unknown rule, a bad `configs`) is answered with an error
  frame carrying its `id`, and the server keeps serving.

If the input ends before any frame, the server exits with status 0.

## Requests

Notifications are applied in order on the thread that reads frames, so a lint request sees every notification sent
before it. Each lint request gets exactly one terminal frame:

- its diagnostic frames, then `Done {"id"}`; or
- one error frame carrying its `id`: a request that cannot be parsed, or a panic while linting. Its diagnostics are
  dropped, so the client keeps its previous results.

Each lint request runs on a thread of its own, with a 512 MB stack, so requests run concurrently. A request takes its
project snapshot when its thread starts. A request's frames are written in one locked write, so the frames of
different requests never interleave.

## Environment

Environment variables are read as in headless mode: `TSRS_FAST=0` turns off the fast preset,
`TSRS_LINT_RUN_EXTERNAL_CODE=1` runs content mappers, and
`OXLINT_TSGOLINT_DANGEROUSLY_SUPPRESS_PROGRAM_DIAGNOSTICS=true` suppresses tsconfig and program errors.

## Shutdown

`Shutdown`, or the end of the input, lets running requests finish and write their frames, closes the project session,
and exits with status 0. After a fatal error, running requests also finish before the server exits.

## File changes

The server does not watch the file system. The client must announce every change on disk that can affect linting
with `FilesChanged`: source files created, changed or deleted, and `tsconfig.json`, `package.json` and other files
that change project configuration or module resolution. Open files are read from their `DidOpen` and `DidChange`
text; after `DidClose`, a file is read from disk again.

Files map to projects by the language server's default-project rules. Files outside every tsconfig are linted in an
inferred project with headless mode's default compiler options.
