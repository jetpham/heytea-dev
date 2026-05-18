# NixOS DigitalOcean Deploy

This is the current production path for `heytea.dev`: create a stock Ubuntu DigitalOcean droplet, install NixOS with `nixos-anywhere`, then use deploy-rs for production updates. OpenTofu is not part of the active production path.

## Prerequisites

Run commands from the repository root with the flake dev shell:

```sh
nix develop "path:$PWD" -c <command>
```

The ignored `.env` file should contain the operational API tokens used for manual bootstrap, including `DIGITALOCEAN_TOKEN` and `CLOUDFLARE_API_TOKEN`.

Runtime secrets are managed with agenix. Required production secrets are:

- `secrets/tailscale-auth-key.age`

## Create The Droplet

Create a normal Ubuntu 24.04 droplet in `sfo3` using the `$4/mo` `1 vCPU`, `512 MB RAM`, `10 GB SSD` size, attach a firewall, and allow only:

- TCP `22` from `0.0.0.0/0` for the public readonly SSH TUI after NixOS is deployed
- TCP `80` from `0.0.0.0/0`
- TCP `443` from `0.0.0.0/0`
- UDP `443` from `0.0.0.0/0` for HTTP/3/QUIC

During the stock Ubuntu bootstrap, TCP `22` is normal admin SSH; restrict it to the operator's current public IP until NixOS is deployed. After the first NixOS deploy, TCP `22` becomes the anonymous readonly SSH TUI and admin SSH moves to Tailscale-only TCP `2222`.

Use the droplet name `heytea-dev`. The production host configuration expects the hostname and Tailscale node name to be `heytea-dev`.

## Install NixOS

Use the installer configuration, which includes the DigitalOcean disk layout and ConfigDrive networking:

```sh
nix develop "path:$PWD" -c nixos-anywhere \
  --flake "path:$PWD#heytea-install" \
  root@<droplet-ip>
```

After the reboot, verify the host is reachable and running NixOS:

```sh
ssh root@<droplet-ip> nixos-version
ssh root@<droplet-ip> hostname
```

## First Production Deploy

The first full deploy can target the public IP while the temporary SSH firewall rule is still open:

```sh
nix --accept-flake-config run "path:$PWD#deploy" -- \
  --hostname <droplet-ip> \
  --ssh-user root
```

This activates `nixosConfigurations.heytea-dev`, starts Postgres/TimescaleDB, migrations, API, poller, MCP server, site, anonymous SSH TUI, Caddy, admin OpenSSH on port `2222`, and Tailscale. Separate metrics dashboards, uptime probes, analytics apps, and DigitalOcean's metrics agent are not part of the minimal production runtime.

## Tailscale Deploys

After Tailscale joins, trust the admin SSH host key over the tailnet:

```sh
ssh -p 2222 -o StrictHostKeyChecking=accept-new root@heytea-dev true
```

Future deploys should use the default deploy-rs target over Tailscale:

```sh
nix --accept-flake-config run "path:$PWD#deploy"
```

The deploy node is defined in `flake.nix` as `deploy.nodes."heytea-dev"` with `sshUser = "root"`.

## DNS

Create Cloudflare proxied web records for:

- `heytea.dev`
- `api.heytea.dev`
- `docs.heytea.dev`
- `mcp.heytea.dev`
- `status.heytea.dev`

Set Cloudflare SSL/TLS mode to `Full (strict)`, keep TLS 1.2 and TLS 1.3 enabled, and enable HTTP/2 and HTTP/3 at the edge. Caddy also enables HTTP/1.1, HTTP/2, HTTP/3/QUIC, TLS 1.2, and TLS 1.3 at the origin.

Raw SSH is not proxied by normal Cloudflare orange-cloud records. Until Cloudflare Spectrum is available, `ssh heytea.dev` only works if `heytea.dev` resolves directly to the droplet. If `heytea.dev` is orange-cloud proxied for web, expose the TUI on a separate DNS-only SSH hostname or expect SSH to fail at Cloudflare.

## Verify

Check system health over Tailscale:

```sh
ssh -p 2222 root@heytea-dev systemctl is-system-running
ssh -p 2222 root@heytea-dev systemctl --failed --no-pager
```

Check public endpoints after DNS propagates:

```sh
curl https://heytea.dev/
curl https://api.heytea.dev/readyz
curl https://api.heytea.dev/locations
curl https://api.heytea.dev/locations/downtown-metreon/status
curl https://docs.heytea.dev/
curl https://mcp.heytea.dev/
curl https://status.heytea.dev/
```

Check the public SSH TUI:

```sh
ssh heytea.dev
```

If the local resolver has stale negative cache immediately after DNS creation, validate against the droplet directly while preserving TLS hostname verification:

```sh
curl --resolve api.heytea.dev:443:<droplet-ip> https://api.heytea.dev/locations/downtown-metreon/status
```

## SSH After Bootstrap

Public TCP `22` is intentionally open for `heytea-ssh-tui`, not admin SSH. Confirm the public TUI and tailnet admin SSH work, and public admin port `2222` is blocked by the firewall:

```sh
ssh heytea.dev
ssh -p 2222 root@heytea-dev true
ssh -p 2222 -o BatchMode=yes -o ConnectTimeout=8 root@<droplet-ip> true
```

## CI/CD

See `docs/ci-cd.md` for the GitHub Actions, Tailscale, deploy key, crates.io Trusted Publisher, and release setup required before enabling production automation.
