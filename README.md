<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/banner-dark.png">
  <img alt="Benchpress" src="docs/assets/banner-light.png">
</picture>

Know what your machine can lift. Benchpress builds real Rust projects with a pinned toolchain, times each build, and shows how your machine compares with others on [benchpress.dev](https://benchpress.dev).

**Run it now, without installing:**

```sh
curl -fsSL https://raw.githubusercontent.com/matx64/benchpress/main/run.sh | sh
```

The command fetches [`run.sh`](run.sh) straight from this repository, so you can read it first.

Linux (x86_64 and arm64) and Apple Silicon Macs. The script downloads one static binary into a temporary directory, verifies its SHA-256 checksum and opens a menu in your terminal. Every action returns to the menu, and Ctrl+C cancels a running benchmark. When you quit, the binary is deleted and Benchpress offers to remove the toolchains and sources it downloaded. It never uses `sudo`.

Pass a command to skip the menu (`… | sh -s -- run quick`), or install the binary to `~/.local/bin` with `… | sh -s -- install`.

<!-- TODO: screenshot of a result page -->

## Suites

| Tier | Project | Typical time | Disk |
| --- | --- | --- | --- |
| `quick` | ripgrep 16.0.0 | 1 to 4 minutes | 2 GB |
| `standard` | rust-analyzer 2026-11-16 | 8 to 30 minutes | 8 GB |
| `heavy` | nushell 0.117.0 | 20 to 90 minutes | 14 GB |

Each suite runs five scenarios three times: a clean debug build, a clean release build, `cargo check`, and two incremental rebuilds after editing a leaf crate and a core crate. The report shows the median of each.

## Commands

```
benchpress                  open the menu
benchpress run <suite>      run quick, standard or heavy
benchpress list             list suites
benchpress results          list local results; `results show <id>` for one
benchpress submit <id>      review the payload, then publish it
benchpress unpublish <id>   delete a published result
benchpress doctor           check the toolchain, disk space and power state
benchpress clean [--all]    delete cached toolchains, sources and builds (--all: results too)
```

Results stay on your machine until you run `submit` or confirm the prompt after a run. A published result has no username, hostname, IP address or file paths. See [docs/methodology.md](docs/methodology.md) for what is measured and how.

## Local data

| What | Linux | macOS |
| --- | --- | --- |
| Toolchains, sources and crates | `~/.cache/benchpress` | `~/Library/Caches/benchpress` |
| Build directories, removed after each run | `/var/tmp/benchpress-<uid>` | `$TMPDIR/benchpress` |
| Results and unpublish tokens | `~/.local/share/benchpress` | `~/Library/Application Support/benchpress` |

`BENCHPRESS_CACHE_DIR` and `BENCHPRESS_DATA_DIR` move the first and last. Manage local data in the menu shows how much each uses and removes them, or delete the folders yourself. Benchpress never touches your own `~/.rustup` or `~/.cargo`.

## Repository

- `crates/core`: result schema, suite manifests, hashing and validation, shared by the CLI and server.
- `crates/cli`: the `benchpress` binary.
- `crates/server`: the website and API, one binary with one SQLite file.
- `suites/`: suite manifests and `suites.lock`.
- `deploy/`: systemd unit, Caddyfile and Litestream config. See [docs/deploy.md](docs/deploy.md).

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.

## License

MIT. See [LICENSE](LICENSE).
