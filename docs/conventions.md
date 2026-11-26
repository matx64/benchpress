# Conventions

## Errors

- `benchpress-core` returns typed errors (`thiserror`). Validation returns a `Rejection` with a stable `code` that the API sends to clients, so never rename a code.
- The binaries use `anyhow` and add context at each boundary: `.context("reading suite manifest")`.
- No `unwrap()` or `expect()` outside tests, except on invariants the code just established. Say which invariant in the `expect` message.

## Output and logging

- The CLI writes to the terminal only through `crates/cli/src/output.rs`. It honors `NO_COLOR` and `--json`, where stdout carries only the JSON document.
- Everything else logs with `tracing`. No `println!` or `eprintln!` outside `output.rs`.
- The server never logs IP addresses, request bodies, query strings or delete tokens. Request logs carry the method, path, status and latency.

## Code

- Every public item in `benchpress-core` has a doc comment. Other comments are rare and explain why, not what.
- `unsafe` is denied workspace-wide. The macOS probes call Objective-C through `objc2`'s safe bindings.
- Clippy runs with `pedantic` and `-D warnings` in CI. Allow a lint in the workspace `Cargo.toml` when it fights the codebase, not with scattered attributes.
- Format with `cargo fmt`.

## Tests

The suite stays small and covers contracts only: suite hashes against `suites.lock`, validation rules, the privacy of a result, isolation from the user's cargo settings, outlier grouping, the API's submit and unpublish flow, and the end-to-end CI job. Add a row to an existing table test before you write a new test. Do not snapshot HTML or SVG.

## Web

- Templates live in `crates/server/templates` and are embedded at build time.
- Colors come only from the `--bp-*` tokens in `static/tokens.css`. Yellow (`--bp-you`) marks the viewer's own result and the focus ring, nothing else.
- Charts are server-rendered SVG with a hidden data table for screen readers. Pages work without JavaScript.
