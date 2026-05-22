# Operations

## Public Domains

- `heytea.dev`
- `api.heytea.dev`
- `docs.heytea.dev`
- `mcp.heytea.dev`
- `status.heytea.dev`

These are intended to be Cloudflare proxied records for web traffic. Caddy terminates origin HTTPS on the host and explicitly supports HTTP/1.1, HTTP/2, HTTP/3/QUIC, TLS 1.2, and TLS 1.3.

`ssh heytea.dev` is direct-to-origin until Cloudflare Spectrum is available. A single hostname cannot be both Cloudflare orange-cloud proxied for web and direct raw SSH without Spectrum, so DNS must either leave `heytea.dev` DNS-only for SSH or expose the TUI on a separate DNS-only SSH hostname.

## SSH

- Public readonly TUI: `ssh ssh.heytea.dev` on TCP `22`, served by `heytea-ssh-tui`, no password or key required.
- Admin SSH: `ssh -p 2222 root@heytea-dev-1` over Tailscale only.

Web domains are Cloudflare-proxied. The origin firewall accepts TCP `80` and `443` only from Cloudflare ranges; direct public web access to the droplet should fail.

## Internal Services

Postgres uses the local Unix socket with peer authentication. It should not bind to Caddy or public interfaces. Separate dashboards, collectors, and analytics apps are not part of the minimal production runtime.

## Tracking

The poller tracks all enabled catalog locations. Wait polling is scheduled independently per upstream provider region and paced by `HEYTEA_WAIT_PROVIDER_MAX_REQUESTS_PER_MINUTE` so each region stays within its configured request rate. Raw observations are stored at exact timestamps, and API history interpolates those observations onto one-minute buckets for current-day and seven-day comparison graphs.

Managed state is still available for operational grouping and notice polling. It is derived from seeded regions plus manual overrides in Postgres:

- Seed regions: San Francisco Bay Area and London.
- Manual admin: `nix run .#admin -- managed list`, `managed add <slug> [note]`, `managed remove <slug>`, `managed recompute`, and `managed regions`.
- Run admin commands on the host over Tailscale admin SSH so the default `DATABASE_URL=postgresql:///heytea?host=/run/postgresql&user=heytea` reaches the local socket.

## Upstream Safety

Catalog refresh defaults to once per day with a slower retry interval after failure. Notices refresh daily, not every minute. Wait polling is batched by provider and paced per region with low upstream concurrency. Upstream `403`, `405`, and `429` responses trip a provider cooldown in `upstream_provider_cooldowns`; the poller skips cooled-down providers instead of retrying every shop individually.

## Backups

Daily restic backups go to Backblaze B2. At minimum, back up Postgres logical dumps.

## Monitoring

`status.heytea.dev` uses live checks from `heytea-site`. Host-level dashboards and self-hosted analytics are intentionally omitted from the minimal runtime; use Cloudflare analytics for web traffic.
