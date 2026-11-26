# AGENTS.md

Benchpress times Rust builds on a user's machine and publishes the results to benchpress.matx.dev. This file is a map. Follow the references into the code, which is the source of truth.

## Checks

Run all three before you commit. CI runs only on published releases, so nothing else catches a failure.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Also run `shellcheck run.sh` when you change `run.sh`. [CONTRIBUTING.md](CONTRIBUTING.md#running-the-site-locally) shows how to run the site locally.

## Layout

| Path | What it holds |
| --- | --- |
| `crates/core` | Shared by CLI and server: result schema, suite manifests, hashing, validation |
| `crates/cli` | The `benchpress` binary |
| `crates/server` | Website and API, one binary with one SQLite file |
| `xtask` | `cargo xtask suites-lock` |
| `suites/` | Suite manifests `<id>@<version>.toml` and `suites.lock` |
| `run.sh` | Launcher that the curl command fetches |
| `Dockerfile`, `fly.toml`, `.github/workflows/` | Server deployment on Fly.io, release pipeline |

## Where things live

Core:

- Result document: `BenchResult` in `crates/core/src/result.rs`, schema version `RESULT_SCHEMA_VERSION`.
- Suite manifest: `SuiteManifest` in `crates/core/src/suite.rs`. Embedded suites and the lockfile: `Registry` in `crates/core/src/registry.rs`.
- Hashes and fingerprints: `crates/core/src/hash.rs`.
- Server-side validation and flags: `validate_result`, `Rejection`, `Flag` and `is_outlier` in `crates/core/src/validate.rs`.
- User-facing advice per flag: `hints` in `crates/core/src/hints.rs`.

CLI:

- Commands: `Command` in `crates/cli/src/main.rs`, handlers in `crates/cli/src/commands/`. The menu is `commands/menu.rs`, local data management is `commands/data.rs`.
- Running a suite: `runner::run` in `crates/cli/src/runner/mod.rs`, timing in `runner/measure.rs`, the clean cargo environment in `runner/env.rs` (`Isolation`).
- Toolchains: `toolchain::ensure` in `crates/cli/src/toolchain/mod.rs`.
- Hardware and OS probes: `crates/cli/src/probe/` (`linux.rs`, `macos.rs`).
- Paths and cache layout: `crates/cli/src/paths.rs`.
- Ctrl+C and signals: `crates/cli/src/interrupt.rs`.
- Terminal output, prompts, `--json`: `crates/cli/src/output.rs`.
- Publishing and unpublishing: `crates/cli/src/submit.rs`.

Server:

- Router, config, rate limit, snapshot refresh: `app`, `Config` and `AppState::refresh` in `crates/server/src/lib.rs`.
- API handlers: `crates/server/src/api.rs`. Storage and migrations: `crates/server/src/db.rs`, `crates/server/migrations/`.
- Aggregates and outlier marking: `Snapshot` and `mark_outliers` in `crates/server/src/stats.rs`.
- Pages: `routes` in `crates/server/src/pages.rs`, templates in `crates/server/templates/`, shared template globals in `crates/server/src/views.rs`.
- SVG charts: `crates/server/src/charts.rs`. Share images: `crates/server/src/og.rs`.
- Static files, `RUN_URL` and `SCRIPT_SOURCE_URL`: `crates/server/src/assets.rs`, `crates/server/static/`.
- Fake data for local work: `crates/server/src/seed.rs`.

Tests: unit tables sit next to the code they cover, plus `crates/cli/tests/pipeline.rs` and `crates/server/tests/api.rs`.

## Rules

- Follow [docs/conventions.md](docs/conventions.md) for errors, output, logging, code style and tests.
- Keep comments to a minimum. The exception is the doc comment every public item in `benchpress-core` needs.
- Never edit a published suite manifest. Add a new version and run `cargo xtask suites-lock`. See [docs/adding-a-suite.md](docs/adding-a-suite.md).
- Never rename a `Rejection` code. Clients depend on them.
- A published result must not gain fields that identify a person or machine: username, hostname, serial number, IP address or file path.
- Keep the test suite small. Add a row to an existing table test before you write a new test, and test contracts only.
- Benchpress runs only through `run.sh`, with nothing installed. Do not add an install path, a manual download or a self-update.
- Update the docs that describe a change in the same commit: [README.md](README.md) for users, [docs/methodology.md](docs/methodology.md) for measurement, validation and comparison, [docs/deploy.md](docs/deploy.md) for the server and releases. The website's about and start pages (`crates/server/templates/about.html`, `start.html`) repeat parts of the methodology and README, so keep them in step.
