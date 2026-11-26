# Deploying the server

The server is one static binary and one SQLite file. Production runs on one Fly.io machine with a volume, at benchpress.matx.dev.

## One-time setup

You need [`flyctl`](https://fly.io/docs/flyctl/install/) and a Fly.io account. From the repository root:

```sh
fly apps create benchpress
fly volumes create benchpress_data --app benchpress --region gru --size 1 --snapshot-retention 30 --yes
fly secrets set --app benchpress --stage BENCHPRESS_ADMIN_TOKEN="$(openssl rand -hex 32)"
fly deploy
fly certs add benchpress.matx.dev --app benchpress
```

`fly certs add` prints the DNS records to create. In Cloudflare, add a `CNAME` from `benchpress` to `benchpress.fly.dev` with the proxy turned off, so Fly can issue the certificate.

The machine stops when no requests arrive and starts on the next one, so the first request after a quiet period takes a second or two. It keeps one SQLite file on the `benchpress_data` volume, so the app runs on exactly one machine. Never scale it out. The server creates the database and applies migrations on start.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `BENCHPRESS_DB` | `benchpress.db` | SQLite file. OG cards are cached next to it in `<name>.og/`. |
| `BENCHPRESS_BIND` | `0.0.0.0:8080` | Listen address. |
| `BENCHPRESS_PUBLIC_URL` | `http://localhost:8080` | Base for share links and OG tags. Set in `fly.toml`. |
| `BENCHPRESS_ADMIN_TOKEN` | unset | A Fly secret. Lets an operator unpublish any result with `DELETE /api/v1/results/<id>`. |
| `BENCHPRESS_LOG_FORMAT` | text | `json` for structured logs. |
| `BENCHPRESS_LOG` | `info` | Log filter, in `RUST_LOG` syntax. |

`Dockerfile` sets `BENCHPRESS_DB`, `BENCHPRESS_BIND` and `BENCHPRESS_LOG_FORMAT` for the container. The submit rate limit (a burst of 10, then one every six minutes, per IPv4 address or IPv6 /64) keys on the `Fly-Client-IP` header that Fly's proxy sets, and on the peer address when the header is missing. Outside Fly, put the server behind a proxy that sets or strips that header, or clients can spoof it. The limiter lives in memory, so it resets each time the machine stops. `BENCHPRESS_ADMIN_TOKEN` must be at least 32 characters or the server refuses to start.

## Releases and deploys

Workflows run only when you publish a GitHub release, so pushes to `main` cost no Actions minutes. Tag each release `v<workspace version>`, for example `v0.1.0` for `version = "0.1.0"` in `Cargo.toml`. The workflow stops when the two differ.

`.github/workflows/release.yml` then:

1. marks the release as a pre-release, so GitHub's "latest" keeps pointing at the previous release while this one builds;
2. runs the CI workflow;
3. builds the CLI for every target and uploads the archives, their checksums, `SHA256SUMS` and a copy of `run.sh` pinned to the new tag;
4. marks the release as latest, unless you published it as a pre-release;
5. calls `.github/workflows/deploy.yml`.

Every release that is not a pre-release ends up as latest, even if you unticked "Set as the latest release". Releases run one at a time. If the tag check or CI fails, the release stays a pre-release without archives. Fix the problem, delete the release and its tag, and publish again.

Deploy runs `fly deploy --remote-only`, which builds `Dockerfile` on Fly's builders and rolls the machine. You can also start it by hand from the Actions tab, or run `fly deploy` locally. It needs the repository secret `FLY_API_TOKEN`, created with `fly tokens create deploy --app benchpress`.

### After the first release

Until a release exists, the run command fetches `run.sh` from `main`, and that script downloads `releases/latest`. Once the first release is out, change the URL to `https://github.com/matx64/benchpress/releases/latest/download/run.sh` in these places, so the script and the binary always come from the same release:

- `RUN_URL` in `crates/server/src/assets.rs`, which the website shows;
- the commands in `README.md`;
- the usage lines at the top of `run.sh`.

## Backups

Fly snapshots the volume once a day and keeps each snapshot for 30 days. Unpublishing deletes a row, so a deleted result leaves the backups 30 days later. The privacy section of the about page says so. To restore, list the snapshots and create a new volume from one:

```sh
fly volumes list --app benchpress
fly volumes snapshots list <volume-id>
fly volumes create benchpress_data --app benchpress --region gru --snapshot-id <snapshot-id>
```

Then destroy the old machine's volume and run `fly deploy`, so the new machine mounts the restored one.

## Monitoring

`fly logs --app benchpress` streams the JSON logs. Alert on lines with `"level":"ERROR"`. The server logs every 500 response at that level. An uptime checker on `https://benchpress.matx.dev/healthz` would keep the machine awake around the clock, so check it no more than every few hours, or not at all.

## Never

Do not run `benchpress-server seed` against production or scale the app past one machine. Seed rows exist for local design work and switch on a banner across the site.
