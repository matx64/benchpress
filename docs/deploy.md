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

Caddy must pass `X-Forwarded-For`, which it does by default. The submit rate limit (10 per hour per client) reads it.

## Deploys

`.github/workflows/deploy.yml` runs after CI passes on `main` when a commit touches `crates/server`, `crates/core`, `suites/`, `install.sh` or `Cargo.lock`. It builds a musl binary, copies it over SSH, restarts the unit and checks `/healthz`. It needs:

- repository variables `DEPLOY_HOST` and `DEPLOY_USER`; the workflow skips itself while `DEPLOY_HOST` is unset,
- secrets `DEPLOY_SSH_KEY` and `DEPLOY_KNOWN_HOSTS`,
- passwordless `sudo` on the host for `install` and `systemctl restart benchpress-server`.

## Backups

Litestream streams every WAL change to S3-compatible storage and keeps 30 days. Test a restore before launch and after any Litestream upgrade:

```sh
litestream restore -config /etc/benchpress/litestream.yml -o /tmp/restore.db /var/lib/benchpress/benchpress.db
sqlite3 /tmp/restore.db 'select count(*) from results'
```

## Monitoring

Point an uptime checker at `https://benchpress.dev/healthz`. With `BENCHPRESS_LOG_FORMAT=json`, alert on log lines with `"level":"ERROR"`; the server logs every 500 response at that level.

## Never

Do not run `benchpress-server seed` against production. Seed rows exist for local design work and switch on a banner across the site.
