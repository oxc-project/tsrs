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
then the UTF-8 JSON, which is always an object. Paths should be absolute; backslashes are accepted. Every path, in
notifications and lint requests alike, is resolved against the server's working directory and normalized (`.` and `..`
segments removed), so one file has one name.

The client sends:

| type | name | JSON |
| --- | --- | --- |
| 16 | `DidOpen` | `{"path", "version", "text"}` |
| 17 | `DidChange` | `{"path", "version", "text"}` (the whole text) |
| 18 | `DidClose` | `{"path"}` |
| 19 | `FilesChanged` | `{"changes": [{"path", "kind"}]}` (LSP `FileChangeType`: 1 created, 2 changed, 3 deleted) |
| 20 | `Lint` | `{"id", "configs": [{"file_paths", "rules"}], "fix", "fix_suggestions"}` |
| 21 | `Shutdown` | `{}` |
| 22 | `Hello` | `{"protocol_version"}` |
| 23 | `Cancel` | `{"id"}` |

The server sends:

| type | name | JSON |
| --- | --- | --- |
| 0 | `Error` | `{"id", "error"}`, plus `"canceled": true` for a canceled request |
| 1 | `Diagnostic` | headless mode's diagnostic object, with the request's `"id"` as its first field |
| 3 | `Done` | `{"id"}` |
| 4 | `Hello` | `{"protocol_version", "version"}` |

`id` is a non-negative integer the client chooses; ids should be unique among the requests in flight. A `Lint`
request's `configs` and `rules` are headless mode's (V2 payload); each file is linted once, with the rules of the
last config that names it, as in headless mode. Compiler diagnostics are not reported. Tsconfig and program errors of
a configured project are reported as in headless mode, on the tsconfig, once per request, and that project's files
are not linted.

## Handshake and versions

The first frame must be the client's `Hello`. The server always answers it with its own `Hello`, for example:

```json
{"protocol_version": 1, "version": "7.1.0-beta.tsrs4"}
```

`protocol_version` is the server's protocol version (`PROTOCOL_VERSION` in `server.rs`) and `version` is the
tsrs-lint version. Versions must match exactly: if the client's `protocol_version` differs, the server sends its
`Hello`, then the fatal error

```json
{"id": null, "error": "protocol version mismatch: client speaks 2, tsrs-lint server speaks 1"}
```

and exits with status 1. A `Hello` without an integer `protocol_version` is also answered with the server's `Hello`
and then a fatal error (`invalid Hello frame: ...`). Any other first frame gets a fatal error
(`expected a Hello frame (message type 22) first, got message type <n>`) and no `Hello`. If the input ends before
any frame, the server exits with status 0.

Any change that an older client or server would misread bumps `PROTOCOL_VERSION`: a new or changed field whose
meaning matters, a changed frame, or changed semantics. Adding a new client notification that an older server may
safely ignore does not need a bump, since the server skips frames of unknown types (below).

## Errors

After the handshake:

- A frame of an unknown message type is skipped, and the server logs
  `tsrs-lint server: skipping unknown message type <n>` to stderr once per type. A second `Hello` is skipped the
  same way.
- A malformed notification (`DidOpen`, `DidChange`, `DidClose`, `FilesChanged`, `Cancel`, `Shutdown`), a `Lint`
  frame without a valid `id`, or a frame that cannot be read is a fatal error: an error frame with a null `id`, after
  which the server writes nothing, not even the terminal frames of queued or running requests (which stop before
  their next file), and exits with status 1. Going on would let the client's view of open files and the server's
  diverge.
- A `Lint` request that cannot be parsed otherwise (an unknown rule, a bad `configs`) is answered with an error
  frame carrying its `id`, and the server keeps serving.

## Requests

Notifications are applied in order on the thread that reads frames, so a lint request sees every notification sent
before it. Each lint request gets exactly one terminal frame, unless a fatal error ends the server first:

- its diagnostic frames, then `Done {"id"}`; or
- one error frame carrying its `id`: a request that cannot be parsed, a panic while linting, or a cancellation. Its
  diagnostics are dropped, so the client keeps its previous results.

A request's frames are written in one locked write, so the frames of different requests never interleave.

## Worker pool

Lint requests wait in a first-in, first-out queue for a fixed pool of worker threads, each with a 512 MB stack. A
running request first loads the projects of its files, one file at a time, then takes one project snapshot and lints
all of its files against it: it sees the notifications that arrive before that snapshot, and none that arrive while
its files are linted. The pool has as many workers as `std::thread::available_parallelism()`, clamped to 1 to 4,
unless `TSRS_LINT_SERVER_WORKERS` is set to a positive integer, which is used as is; other values are ignored.

Other environment variables read as in headless mode: `TSRS_FAST=0` turns off the fast preset,
`TSRS_LINT_RUN_EXTERNAL_CODE=1` runs content mappers, and
`OXLINT_TSGOLINT_DANGEROUSLY_SUPPRESS_PROGRAM_DIAGNOSTICS=true` suppresses tsconfig and program errors.

## Cancellation

`Cancel {"id"}` cancels a request:

- A queued request is removed from the queue and answered at once.
- A running request stops before it loads the project of its next file, or before it lints its next file (a
  program being built is not interrupted). It drops its diagnostics and is answered instead of `Done`. A request
  canceled after its last file is still answered as canceled if it has not written its frames yet.
- A finished or unknown `id` is ignored: no frame is sent.

A canceled request's terminal frame is:

```json
{"id": 7, "error": "request canceled", "canceled": true}
```

Clients should recognize cancellation by `"canceled": true`, not by the error text.

## Shutdown

`Shutdown`, or the end of the input, answers every queued request as canceled, lets running requests finish and
write their frames, closes the project session, and exits with status 0. After a fatal error, which is the last frame,
queued and running requests are not answered.

## File changes

The server does not watch the file system. The client must announce every change on disk that can affect linting
with `FilesChanged`: source files created, changed or deleted, and `tsconfig.json`, `package.json` and other files
that change project configuration or module resolution. Open files are read from their `DidOpen` and `DidChange`
text; after `DidClose`, a file is read from disk again.

Files map to projects by the language server's default-project rules. Files outside every tsconfig are linted in an
inferred project with headless mode's default compiler options.
