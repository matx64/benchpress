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

The submit rate limit (10 per hour per IPv4 address or IPv6 /64) reads the client IP from `X-Forwarded-For`. Caddy 2.5 and later replace any `X-Forwarded-For` a client sends unless you configure `trusted_proxies`, so clients cannot spoof it. Do not expose port 8080 directly. `BENCHPRESS_ADMIN_TOKEN` must be at least 32 characters or the server refuses to start; generate one with `openssl rand -hex 32`.

## Deploys

Workflows run only when you publish a GitHub release, so day-to-day pushes cost no Actions minutes. `.github/workflows/release.yml` runs CI, builds the CLI for every target, uploads the archives to the release, and then calls `.github/workflows/deploy.yml`. Deploy builds a musl server binary from the release tag, copies it over SSH, restarts the unit and checks `/healthz`. You can also run Deploy by hand from the Actions tab. It needs:

- repository variables `DEPLOY_HOST` and `DEPLOY_USER`; deploy skips itself while `DEPLOY_HOST` is unset,
- secrets `DEPLOY_SSH_KEY` and `DEPLOY_KNOWN_HOSTS`,
- passwordless `sudo` on the host for `install` and `systemctl restart benchpress-server`.

Tag releases `v<workspace version>` (for example `v0.1.0` for `version = "0.1.0"` in `Cargo.toml`); the workflow stops when they differ. The server serves `run.sh` pinned to the release it was built from, so `curl … /run | sh` keeps downloading the previous release while a new one is tested and built, and switches once deploy finishes. Run `run.sh` from the repository and it falls back to `releases/latest`. If CI fails, the release stays without archives: fix the problem, delete the release and its tag, and publish again.

## Backups

Litestream streams every WAL change to S3-compatible storage and keeps 30 days. Unpublishing deletes a row, so a deleted result disappears from backups once its 30 days pass; the privacy section of the about page says so. Test a restore before launch and after any Litestream upgrade:

```sh
litestream restore -config /etc/benchpress/litestream.yml -o /tmp/restore.db /var/lib/benchpress/benchpress.db
sqlite3 /tmp/restore.db 'select count(*) from results'
```

## Monitoring

Point an uptime checker at `https://benchpress.dev/healthz`. With `BENCHPRESS_LOG_FORMAT=json`, alert on log lines with `"level":"ERROR"`; the server logs every 500 response at that level.

## Never

Do not run `benchpress-server seed` against production. Seed rows exist for local design work and switch on a banner across the site.
