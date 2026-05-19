# CI/CD Setup

GitHub is intended to be the source of truth for `jetpham/heytea-dev`. The local `main` branch should be pushed to `origin/main` after the required repository settings and secrets are in place.

## Workflows

- `CI`: runs on pull requests and pushes to `main`.
- `Deploy`: runs after successful `CI` on `main`, configures Cloudflare DNS/proxying, deploys NixOS, and verifies production.
- `Publish Crate`: runs on pushes to `main`, waits for the matching `CI` run to pass, and publishes the `heytea` Rust SDK when the package version is not already on crates.io.
- `Release CLI`: runs after successful `CI` on `main`; creates `heytea-cli-v<heytea-cli version>` and uploads the Linux CLI tarball when that release does not already exist.

## GitHub Repository Settings

Required:

- Default branch: `main`.
- Actions enabled.
- Environment `production` for deployments.
- Environment `crates-io` for crates.io Trusted Publishing, with no manual approval gate.

Recommended:

- Require `CI` before merging to `main`.
- Keep `contents: write` limited to release workflows only.

No GitHub Actions variables are required right now.

## GitHub Secrets

Required for deploys:

- `TS_OAUTH_CLIENT_ID`: Tailscale OAuth client ID used by `tailscale/github-action`.
- `TS_OAUTH_SECRET`: Tailscale OAuth client secret.
- `DEPLOY_SSH_KEY`: private Ed25519 key allowed to SSH as `root` to `heytea-dev-1` over Tailscale.
- `CLOUDFLARE_API_TOKEN`: Cloudflare token allowed to read zones, edit DNS records, and edit zone SSL settings for `heytea.dev`.

Do not add a static crates.io token if Trusted Publishing is configured. The SDK publish workflow uses GitHub OIDC through `rust-lang/crates-io-auth-action`.

## Tailscale

The deploy workflow joins the tailnet as `tag:ci` and deploys to the server node tagged `tag:server`.

Tailscale requirements:

- OAuth client can create tagged ephemeral nodes with `tag:ci`.
- ACL permits `tag:ci` to connect to `tag:server` on TCP `2222`.
- `tag:ci` has a tag owner in the tailnet policy.
- `tag:server` has a tag owner in the tailnet policy.
- MagicDNS resolves `heytea-dev-1`, or the workflow should be changed to use the current Tailscale IPv4 directly.

Minimal ACL shape:

```json
{
  "tagOwners": {
    "tag:ci": ["autogroup:admin"],
    "tag:server": ["autogroup:admin"]
  },
  "acls": [
    { "action": "accept", "src": ["tag:ci"], "dst": ["tag:server:2222"] }
  ]
}
```

Current production server state checked from the operator machine:

- Tailscale host: `heytea-dev-1`
- Tailscale IPv4: `100.109.239.16`
- Server tag: `tag:server`

## Deploy SSH Key

Use a dedicated deploy key rather than a personal SSH key.

Steps:

1. Generate an Ed25519 keypair outside the repo.
2. Add the public key to `users.users.root.openssh.authorizedKeys.keys` in `nix/hosts/heytea.nix`.
3. Deploy once manually from the operator machine so the server trusts the new public key.
4. Add the private key as the GitHub secret `DEPLOY_SSH_KEY`.
5. Verify from GitHub Actions with a manual `Deploy` run.

The deploy workflow uses deploy-rs against flake node `.#heytea-dev`, SSH user `root`, host `heytea-dev-1`, and port `2222` over Tailscale. Public TCP `22` is reserved for the anonymous readonly SSH TUI.

## Runtime Secrets

Runtime secrets are encrypted with agenix and committed as `.age` files. They are not GitHub Actions secrets.

Current encrypted runtime secrets:

- `secrets/tailscale-auth-key.age`

These must remain decryptable by the production host key in `secrets/secrets.nix`.

## Cloudflare Proxying

Production deploys configure these A records automatically:

- Proxied through Cloudflare: `heytea.dev`, `api.heytea.dev`, `docs.heytea.dev`, `mcp.heytea.dev`, `status.heytea.dev`.
- DNS-only direct SSH TUI: `ssh.heytea.dev`.

The deploy workflow also sets the zone SSL mode to `Full`. Caddy uses an internal origin certificate because public HTTPS is only intended to enter through Cloudflare.

The NixOS host firewall keeps TCP `22` public for the anonymous readonly SSH TUI, allows TCP `80` and `443` only from Cloudflare source ranges, and trusts `tailscale0` for admin/deploy access.

After each deploy, GitHub Actions verifies that Cloudflare is serving `https://heytea.dev/`, that direct origin HTTPS is blocked, and that the site can resolve `8.8.8.8` to Mountain View for server-side GeoIP sorting.

## GeoIP Database

The DB-IP City Lite MMDB is downloaded and pinned by `flake.nix` as `.#dbip-city-lite-mmdb`. Production points `HEYTEA_GEOIP_MMDB` at that Nix store path automatically; no server-side file copy is needed.

## crates.io SDK Publishing

The SDK package is `heytea`.

Required crates.io Trusted Publisher settings:

- Repository: `jetpham/heytea-dev`
- Workflow: `publish-crate.yml`
- Environment: `crates-io`
- Package: `heytea`

Main-branch behavior:

- On pushes to `main`, the workflow waits for the matching `CI` run to pass and then checks the `heytea` version from `crates/sdk/Cargo.toml`. This stays on the `push` event because crates.io Trusted Publishing does not support `workflow_run`.
- If that exact version is already on crates.io, the workflow exits successfully without publishing.
- If that version is not on crates.io, the workflow runs `cargo publish -p heytea --dry-run --locked`, authenticates with crates.io Trusted Publishing, and publishes.
- Bump `crates/sdk/Cargo.toml` before merging a new SDK release to `main`.
- `v0.1.0` was already published.

## CLI Releases

The CLI release workflow builds the Nix package `.#heytea-cli` and uploads:

- `heytea-<version>-x86_64-linux.tar.gz`
- `SHA256SUMS`

Main-branch behavior:

- After successful `CI` on `main`, the workflow reads the `heytea-cli` version from `crates/cli/Cargo.toml`.
- If release `heytea-cli-v<version>` already exists, the workflow exits successfully without rebuilding assets.
- If release `heytea-cli-v<version>` does not exist, the workflow builds `.#heytea-cli`, uploads the workflow artifact, and creates the GitHub release with the tarball plus `SHA256SUMS` attached during release creation.
- Manual dispatch remains available for new release tags. Existing releases are treated as immutable and skipped.

The current release artifact target is Linux x86_64. Additional portable/static or macOS/Windows artifacts can be added later.

## Infrastructure Tokens

The deploy workflow provisions Cloudflare DNS/proxy state. These additional tokens are only needed for manual infrastructure changes:

- `DIGITALOCEAN_TOKEN`
- Backblaze B2 credentials for restic setup, if changing backup infrastructure.

## First Push Checklist

Before pushing `main`:

- Add GitHub environments: `production`, `crates-io`.
- Add GitHub secrets: `TS_OAUTH_CLIENT_ID`, `TS_OAUTH_SECRET`, `DEPLOY_SSH_KEY`, `CLOUDFLARE_API_TOKEN`.
- Confirm Tailscale ACL allows `tag:ci` to SSH to `tag:server:2222`.
- Confirm the deploy SSH public key is already present on the server through NixOS config.
- Decide the production database cutover plan for the fresh location-first schema.
- Configure crates.io Trusted Publisher for `heytea`.

After pushing:

- Confirm `CI` passes on `main`.
- Confirm `Deploy` completes successfully.
- Verify `https://api.heytea.dev/locations` and one slugged status endpoint.
