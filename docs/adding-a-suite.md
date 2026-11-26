# Adding a suite

A suite is a TOML manifest in `suites/`, named `<id>@<version>.toml`. The CLI and server embed every manifest at build time, so a release always runs the same suites.

## Pick a project

- It builds with the pinned stable toolchain on Linux x86_64, Linux aarch64 and macOS aarch64.
- It needs no system libraries. OpenSSL, GTK and similar dependencies rule a project out. A C compiler from macOS Command Line Tools or Linux `build-essential` is fine.
- Its build script downloads nothing. Cargo runs with `--offline` during timing.
- It has a `Cargo.lock` at the pinned commit.

## Write the manifest

Copy an existing manifest and change:

- `id`, `tier`, `display_name` and `description`.
- `[source]`: the repository URL, the full 40-character commit, and the SHA-256 of `https://github.com/<org>/<repo>/archive/<commit>.tar.gz`. Get the hash with `curl -fsSL <url> | sha256sum`.
- `[build] package` when the workspace root does not build the binary you want.
- The `touch` paths of the two incremental scenarios. `incremental-leaf` edits a file only the final binary depends on. `incremental-core` edits a library that most of the workspace depends on.
- `min_wall_ms` per scenario: about a third of the fastest time you expect on a fast desktop. The server rejects runs faster than this.
- `[requirements] disk_gb` and `estimated_minutes`.

## Test it

```sh
cargo run -p benchpress -- run --suite-file suites/<id>@<version>.toml --runs 1 --cooldown 0 --no-submit
```

Run it on each supported platform. The manual `Suites` workflow builds a suite on GitHub's Linux arm64 and macOS runners.

## Lock it

```sh
cargo xtask suites-lock
```

This rewrites `suites/suites.lock`. A test fails when a manifest's hash differs from the lockfile, so any change to an existing suite needs a new `version` and a new file. Never edit a published manifest in place: results from the old version would no longer match their suite.
