<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/banner-dark.png">
  <img alt="Benchpress" src="docs/assets/banner-light.png">
</picture>

###

Benchpress measures how fast your computer compiles software. It builds a well-known open source project several ways, from a clean release build to a one-line incremental rebuild, and times each one. Every machine builds the same source with the same compiler version, so the times are comparable. You can publish your result to [benchpress.dev](https://benchpress.dev) and see how your machine ranks against others with the same CPU, memory or OS.

Use it to compare laptops before you buy, check whether an upgrade paid off, or see how a new machine stacks up before you trust it with real work.

## Run it, nothing to install

```sh
curl -fsSL https://raw.githubusercontent.com/matx64/benchpress/main/run.sh | sh
```

This works on Linux (x86_64 and arm64) and Apple Silicon Macs, and never uses `sudo`. The command fetches [`run.sh`](run.sh) from this repository, so you can read it first. The script:

1. downloads one static binary from the latest release into a temporary directory and checks its SHA-256;
2. opens a menu in your terminal, where every action returns to the menu and Ctrl+C cancels a running benchmark;
3. deletes the binary when you quit, after offering to remove the toolchains and sources it downloaded.

To skip the menu, pass a command after `sh -s --`:

```sh
curl -fsSL https://raw.githubusercontent.com/matx64/benchpress/main/run.sh | sh -s -- run quick
```

<!-- TODO: screenshot of a result page -->

## Suites

| Tier | Project | Typical time | Disk |
| --- | --- | --- | --- |
| `quick` | ripgrep 16.0.0 | 1 to 4 minutes | 2 GB |
| `standard` | rust-analyzer 2026-11-16 | 8 to 30 minutes | 8 GB |
| `heavy` | nushell 0.117.0 | 20 to 90 minutes | 14 GB |

Each suite runs five scenarios three times: a clean debug build, a clean release build, `cargo check`, and two incremental rebuilds after editing a leaf crate and a core crate. The report shows the median of each. [docs/methodology.md](docs/methodology.md) explains what is measured and how the site compares results.

## Commands

The menu covers everything. To run one command without it, pass the command after `sh -s --`:

```
run <suite>        run quick, standard or heavy
list               list suites
results            list local results; `results show <id>` for one
submit <id>        review the payload, then publish it
unpublish <id>     delete a published result
doctor             check the toolchain, disk space and power state
clean [--all]      delete cached toolchains, sources and builds (--all: results too)
```

## Privacy

Results stay on your machine until you run `submit` or say yes to the prompt after a run. A published result contains no username, hostname, IP address or file path. You can unpublish it later from the machine that published it, as long as its local results folder still holds the unpublish token. `clean --all` and removing results in the menu delete the token.

## Local data

| What | Linux | macOS |
| --- | --- | --- |
| Toolchains, sources and crates | `~/.cache/benchpress` | `~/Library/Caches/benchpress` |
| Build directories, removed after each run | `/var/tmp/benchpress-<uid>` | `$TMPDIR/benchpress` |
| Results and unpublish tokens | `~/.local/share/benchpress` | `~/Library/Application Support/benchpress` |

`BENCHPRESS_CACHE_DIR` and `BENCHPRESS_DATA_DIR` move the first and last. "Manage local data" in the menu shows how much space each one uses and removes it. You can also delete the folders yourself. Benchpress never touches your own `~/.rustup` or `~/.cargo`.

## Contributing

[CONTRIBUTING.md](CONTRIBUTING.md) covers setup, checks and running the site locally. Coding agents start at [AGENTS.md](AGENTS.md).

## License

MIT. See [LICENSE](LICENSE).
