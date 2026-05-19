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

Use the droplet name `heytea-dev`. The production host configuration uses `heytea-dev`; the active Tailscale MagicDNS deploy target is `heytea-dev-1` while the retired node name remains reserved.

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
ssh -p 2222 -o StrictHostKeyChecking=accept-new root@heytea-dev-1 true
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

The deploy workflow sets Cloudflare SSL/TLS mode to `Full`, keeps web records orange-cloud proxied, and keeps `ssh.heytea.dev` DNS-only for the SSH TUI. Caddy uses an internal origin certificate because public web traffic is only intended to enter through Cloudflare.

Raw SSH is not proxied by normal Cloudflare orange-cloud records. Use `ssh ssh.heytea.dev` for the public TUI.

## Verify

Check system health over Tailscale:

```sh
ssh -p 2222 root@heytea-dev-1 systemctl is-system-running
ssh -p 2222 root@heytea-dev-1 systemctl --failed --no-pager
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
ssh ssh.heytea.dev
```

If the local resolver has stale negative cache immediately after DNS creation, validate against the droplet directly while preserving TLS hostname verification:

```sh
curl --resolve api.heytea.dev:443:<droplet-ip> https://api.heytea.dev/locations/downtown-metreon/status
```

## SSH After Bootstrap

Public TCP `22` is intentionally open for `heytea-ssh-tui`, not admin SSH. Confirm the public TUI and tailnet admin SSH work, and public admin port `2222` is blocked by the firewall:

```sh
ssh ssh.heytea.dev
ssh -p 2222 root@heytea-dev-1 true
ssh -p 2222 -o BatchMode=yes -o ConnectTimeout=8 root@<droplet-ip> true
```

## CI/CD

See `docs/ci-cd.md` for the GitHub Actions, Tailscale, deploy key, crates.io Trusted Publisher, and release setup required before enabling production automation.
