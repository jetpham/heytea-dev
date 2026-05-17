# NixOS DigitalOcean Deploy

This is the current production path for `heytea.dev`: create a stock Ubuntu DigitalOcean droplet, install NixOS with `nixos-anywhere`, then use deploy-rs for production updates. The custom image and OpenTofu files under `infra/opentofu` are retained as legacy/manual reference only.

## Prerequisites

Run commands from the repository root with the flake dev shell:

```sh
nix develop "path:$PWD" -c <command>
```

The ignored `.env` file should contain the operational API tokens used for manual bootstrap, including `DIGITALOCEAN_TOKEN` and `CLOUDFLARE_API_TOKEN`.

Runtime secrets are managed with agenix. Required production secrets are:

- `secrets/tailscale-auth-key.age`
- `secrets/grafana-secret-key.age`
- `secrets/grafana-admin-password.age`
- `secrets/umami-app-secret.age`

## Create The Droplet

Create a normal Ubuntu 24.04 droplet in `sfo3`, attach a firewall, and allow only:

- TCP `80` from `0.0.0.0/0`
- TCP `443` from `0.0.0.0/0`
- TCP `22` temporarily from the operator's current public `/32`

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

This activates `nixosConfigurations.heytea-dev`, starts the API, poller, MCP server, site, Caddy, Postgres/TimescaleDB, Grafana, Umami, and Tailscale.

## Tailscale Deploys

After Tailscale joins, trust the host key over the tailnet:

```sh
ssh -o StrictHostKeyChecking=accept-new root@heytea-dev true
```

Future deploys should use the default deploy-rs target over Tailscale:

```sh
nix --accept-flake-config run "path:$PWD#deploy"
```

The deploy node is defined in `flake.nix` as `deploy.nodes."heytea-dev"` with `sshUser = "root"`.

## DNS

Create DNS-only Cloudflare `A` records pointing at the droplet IPv4 for:

- `heytea.dev`
- `api.heytea.dev`
- `docs.heytea.dev`
- `mcp.heytea.dev`
- `analytics.heytea.dev`
- `status.heytea.dev`

Keep these records DNS-only unless Caddy and ACME challenge handling are intentionally changed. Caddy terminates public HTTPS on the droplet.

## Verify

Check system health over Tailscale:

```sh
ssh root@heytea-dev systemctl is-system-running
ssh root@heytea-dev systemctl --failed --no-pager
```

Check public endpoints after DNS propagates:

```sh
curl https://heytea.dev/
curl https://api.heytea.dev/readyz
curl https://api.heytea.dev/locations
curl https://api.heytea.dev/locations/downtown-metreon/status
curl https://docs.heytea.dev/
curl https://mcp.heytea.dev/
curl https://analytics.heytea.dev/
curl https://status.heytea.dev/
```

If the local resolver has stale negative cache immediately after DNS creation, validate against the droplet directly while preserving TLS hostname verification:

```sh
curl --resolve api.heytea.dev:443:<droplet-ip> https://api.heytea.dev/locations/downtown-metreon/status
```

## Lock Down SSH

Once deploy-rs and SSH work over Tailscale, remove public TCP `22` from the DigitalOcean firewall:

```sh
nix develop "path:$PWD" -c doctl compute firewall remove-rules <firewall-id> \
  --access-token "$DIGITALOCEAN_TOKEN" \
  --inbound-rules "protocol:tcp,ports:22,address:<operator-public-ip>/32"
```

Confirm public SSH is blocked and tailnet SSH still works:

```sh
ssh root@heytea-dev true
ssh -o BatchMode=yes -o ConnectTimeout=8 root@<droplet-ip> true
```

## CI/CD

See `docs/ci-cd.md` for the GitHub Actions, Tailscale, deploy key, crates.io Trusted Publisher, and release setup required before enabling production automation.
