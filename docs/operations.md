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

- Public readonly TUI: `ssh heytea.dev` on TCP `22`, served by `heytea-ssh-tui`, no password or key required.
- Admin SSH: `ssh -p 2222 root@heytea-dev` over Tailscale only.

## Internal Services

Postgres uses the local Unix socket with peer authentication. It should not bind to Caddy or public interfaces. Separate dashboards, collectors, and analytics apps are not part of the minimal production runtime.

## Backups

Daily restic backups go to Backblaze B2. At minimum, back up Postgres logical dumps.

## Monitoring

`status.heytea.dev` uses live checks from `heytea-site`. Host-level dashboards and self-hosted analytics are intentionally omitted from the minimal runtime; use Cloudflare analytics for web traffic.
