# Contributing

## Setup

`rust-toolchain.toml` pins the toolchain, so `rustup` installs it on the first `cargo` command.

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

CI does not run on pushes or pull requests. Run the three commands above before you push. Publishing a GitHub release runs the full CI workflow (the same commands on Linux and macOS, `cargo deny check`, a test of `run.sh`, and an end-to-end job that runs the quick suite and publishes it to a local server) before it builds and uploads the binaries. Maintainers can also start CI by hand from the Actions tab.

## Running the site locally

```sh
BENCHPRESS_DB=dev.db cargo run -p benchpress-server -- seed --count 40
BENCHPRESS_DB=dev.db cargo run -p benchpress-server
```

Open http://localhost:8080. Seed results are fake and the site shows a banner while any exist. Point the CLI at your local server with `BENCHPRESS_SERVER=http://localhost:8080`.

## Changes

- Read [docs/conventions.md](docs/conventions.md).
- New suites follow [docs/adding-a-suite.md](docs/adding-a-suite.md). Never change a published manifest; add a new version.
- A change to measured behavior updates [docs/methodology.md](docs/methodology.md) in the same pull request.
- Tests cover contracts only. Extend an existing table test before writing a new one.

Contributions are licensed under MIT, like the rest of the project.
