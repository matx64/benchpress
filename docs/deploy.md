# Deploying the server

The server is one static binary and one SQLite file. Production runs on a small VPS with Caddy for TLS and Litestream for continuous backups.

<!-- TODO(decision): hosting provider and final domain. The defaults below assume a Hetzner-class VPS and benchpress.dev. -->

## One-time setup

On a fresh Debian or Ubuntu host:

```sh
sudo apt install caddy
curl -fsSL -o litestream.deb https://github.com/benbjohnson/litestream/releases/download/v0.3.13/litestream-v0.3.13-linux-amd64.deb
sudo dpkg -i litestream.deb
sudo useradd --system --home /var/lib/benchpress --shell /usr/sbin/nologin benchpress
sudo install -d -o benchpress -g benchpress /var/lib/benchpress
sudo install -d /etc/benchpress
```

Copy the files from `deploy/`:

| File | Destination |
| --- | --- |
| `benchpress-server.service` | `/etc/systemd/system/benchpress-server.service` |
| `litestream.yml` | `/etc/benchpress/litestream.yml` |
| `env.example` | `/etc/benchpress/env`, filled in and `chmod 600` |
| `Caddyfile` | `/etc/caddy/Caddyfile` |

Build the binary with `cargo build -p benchpress-server --release --target x86_64-unknown-linux-musl`, copy it to `/usr/local/bin/benchpress-server`, then:

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now benchpress-server
sudo systemctl reload caddy
curl -fsS http://127.0.0.1:8080/healthz
```

The unit runs the server under `litestream replicate -exec`, so replication starts before the server and stops after it. The server creates the database and applies migrations on start.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `BENCHPRESS_DB` | `benchpress.db` | SQLite file. OG cards are cached next to it in `<name>.og/`. |
| `BENCHPRESS_BIND` | `0.0.0.0:8080` | Listen address. |
| `BENCHPRESS_PUBLIC_URL` | `http://localhost:8080` | Base for share links and OG tags. |
| `BENCHPRESS_ADMIN_TOKEN` | unset | Lets an operator unpublish any result with `DELETE /api/v1/results/<id>`. |
| `BENCHPRESS_LOG_FORMAT` | text | `json` for structured logs. |
| `BENCHPRESS_LOG` | `info` | Log filter, in `RUST_LOG` syntax. |

The submit rate limit (a burst of 10, then one every six minutes, per IPv4 address or IPv6 /64) reads the client IP from `X-Forwarded-For`. Caddy 2.5 and later replace any `X-Forwarded-For` a client sends unless you configure `trusted_proxies`, so clients cannot spoof it. Do not expose port 8080 directly. `BENCHPRESS_ADMIN_TOKEN` must be at least 32 characters or the server refuses to start; generate one with `openssl rand -hex 32`.

## Releases and deploys

Workflows run only when you publish a GitHub release, so pushes to `main` cost no Actions minutes. Tag each release `v<workspace version>`, for example `v0.1.0` for `version = "0.1.0"` in `Cargo.toml`. The workflow stops when the two differ.

`.github/workflows/release.yml` then:

1. marks the release as a pre-release, so GitHub's "latest" keeps pointing at the previous release while this one builds;
2. runs the CI workflow;
3. builds the CLI for every target and uploads the archives, their checksums, `SHA256SUMS` and a copy of `run.sh` pinned to the new tag;
4. marks the release as latest, unless you published it as a pre-release;
5. calls `.github/workflows/deploy.yml`.

Every release that is not a pre-release ends up as latest, even if you unticked "Set as the latest release". Releases run one at a time. If the tag check or CI fails, the release stays a pre-release without archives. Fix the problem, delete the release and its tag, and publish again.

Deploy builds a musl server binary from the release tag, copies it over SSH, restarts the unit and checks `/healthz`. You can also start it by hand from the Actions tab. It needs:

- repository variables `DEPLOY_HOST` and `DEPLOY_USER` (deploy skips itself while `DEPLOY_HOST` is unset);
- secrets `DEPLOY_SSH_KEY` and `DEPLOY_KNOWN_HOSTS`;
- passwordless `sudo` on the host for `install` and `systemctl restart benchpress-server`.

### After the first release

Until a release exists, the run command fetches `run.sh` from `main`, and that script downloads `releases/latest`. Once the first release is out, change the URL to `https://github.com/matx64/benchpress/releases/latest/download/run.sh` in these places, so the script and the binary always come from the same release:

- `RUN_URL` in `crates/server/src/assets.rs`, which the website shows;
- the commands in `README.md`;
- the usage lines at the top of `run.sh`.

## Backups

Litestream streams every WAL change to S3-compatible storage and keeps 30 days. Unpublishing deletes a row, so a deleted result leaves the backups 30 days later. The privacy section of the about page says so. Test a restore before launch and after any Litestream upgrade:

```sh
litestream restore -config /etc/benchpress/litestream.yml -o /tmp/restore.db /var/lib/benchpress/benchpress.db
sqlite3 /tmp/restore.db 'select count(*) from results'
```

## Monitoring

Point an uptime checker at `https://benchpress.dev/healthz`. With `BENCHPRESS_LOG_FORMAT=json`, alert on log lines with `"level":"ERROR"`. The server logs every 500 response at that level.

## Never

Do not run `benchpress-server seed` against production. Seed rows exist for local design work and switch on a banner across the site.
