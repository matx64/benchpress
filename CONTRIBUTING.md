# Contributing

## Setup

`rust-toolchain.toml` pins the toolchain, so `rustup` installs it on the first `cargo` command. Run these before every push:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI does not run on pushes or pull requests. It runs when a maintainer publishes a GitHub release, or by hand from the Actions tab. It repeats the three commands on Linux and macOS, runs `cargo deny check` and `shellcheck run.sh`, tests `run.sh` against a local mirror, and runs the quick suite end to end against a local server.

## Running the site locally

```sh
BENCHPRESS_DB=dev.db cargo run -p benchpress-server -- seed --count 40
BENCHPRESS_DB=dev.db cargo run -p benchpress-server
```

Open http://localhost:8080. Seed results are fake, and the site shows a banner while any exist. To point the CLI at this server, set `BENCHPRESS_SERVER=http://localhost:8080`.

## Making changes

- Follow [docs/conventions.md](docs/conventions.md).
- Add suites as described in [docs/adding-a-suite.md](docs/adding-a-suite.md). Never change a published manifest. Add a new version instead.
- A change to what a run measures, or to how the server validates or compares results, updates [docs/methodology.md](docs/methodology.md) in the same pull request.
- Server, release and deploy changes update [docs/deploy.md](docs/deploy.md).
- Tests cover contracts only. Add a row to an existing table test before you write a new one.

Contributions are licensed under MIT, like the rest of the project.
