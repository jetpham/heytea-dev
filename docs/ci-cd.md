# CI/CD Setup

GitHub is intended to be the source of truth for `jetpham/heytea-dev`. The local `main` branch should be pushed to `origin/main` after the required repository settings and secrets are in place.

## Workflows

- `CI`: runs on pull requests and pushes to `main`.
- `Deploy`: runs after successful `CI` on `main`, and can also be run manually.
- `Publish Crate`: publishes the `heytea` Rust SDK from a GitHub release or manual dispatch.
- `Release CLI`: builds `.#heytea-cli`, packages `heytea-<version>-x86_64-linux.tar.gz`, writes `SHA256SUMS`, and uploads both to a GitHub release.

## GitHub Repository Settings

Required:

- Default branch: `main`.
- Actions enabled.
- Environment `production` for deployments.
- Environment `crates-io` for crates.io Trusted Publishing.

Recommended:

- Require `CI` before merging to `main`.
- Add manual approval protection to the `production` environment until deploys are proven stable.
- Keep `contents: write` limited to release workflows only.

No GitHub Actions variables are required right now.

## GitHub Secrets

Required for deploys:

- `TS_OAUTH_CLIENT_ID`: Tailscale OAuth client ID used by `tailscale/github-action`.
- `TS_OAUTH_SECRET`: Tailscale OAuth client secret.
- `DEPLOY_SSH_KEY`: private Ed25519 key allowed to SSH as `root` to `heytea-dev` over Tailscale.

Do not add a static crates.io token if Trusted Publishing is configured. The SDK publish workflow uses GitHub OIDC through `rust-lang/crates-io-auth-action`.

## Tailscale

The deploy workflow joins the tailnet as `tag:ci` and deploys to the server node tagged `tag:server`.

Tailscale requirements:

- OAuth client can create tagged ephemeral nodes with `tag:ci`.
- ACL permits `tag:ci` to connect to `tag:server` on TCP `2222`.
- `tag:ci` has a tag owner in the tailnet policy.
- `tag:server` has a tag owner in the tailnet policy.
- MagicDNS resolves `heytea-dev`, or the workflow should be changed to use `100.86.76.122` directly.

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

- Tailscale host: `heytea-dev`
- Tailscale IPv4: `100.86.76.122`
- Server tag: `tag:server`

## Deploy SSH Key

Use a dedicated deploy key rather than a personal SSH key.

Steps:

1. Generate an Ed25519 keypair outside the repo.
2. Add the public key to `users.users.root.openssh.authorizedKeys.keys` in `nix/hosts/heytea.nix`.
3. Deploy once manually from the operator machine so the server trusts the new public key.
4. Add the private key as the GitHub secret `DEPLOY_SSH_KEY`.
5. Verify from GitHub Actions with a manual `Deploy` run.

The deploy workflow uses deploy-rs against flake node `.#heytea-dev`, SSH user `root`, host `heytea-dev`, and port `2222` over Tailscale. Public TCP `22` is reserved for the anonymous readonly SSH TUI.

## Runtime Secrets

Runtime secrets are encrypted with agenix and committed as `.age` files. They are not GitHub Actions secrets.

Current encrypted runtime secrets:

- `secrets/tailscale-auth-key.age`

These must remain decryptable by the production host key in `secrets/secrets.nix`.

## crates.io SDK Publishing

The SDK package is `heytea`.

Required crates.io Trusted Publisher settings:

- Repository: `jetpham/heytea-dev`
- Workflow: `publish-crate.yml`
- Environment: `crates-io`
- Package: `heytea`

Release behavior:

- Publishing from a GitHub release requires the release tag to match the SDK package version, such as `v0.1.1` for version `0.1.1`.
- Bump `crates/sdk/Cargo.toml` before creating a release for a new SDK publish.
- `v0.1.0` was already published, so the next SDK publish needs a higher version.

## CLI Releases

The CLI release workflow builds the Nix package `.#heytea-cli` and uploads:

- `heytea-<version>-x86_64-linux.tar.gz`
- `SHA256SUMS`

Release behavior:

- On a published GitHub release, assets are uploaded to that release.
- On manual dispatch, provide `tag_name`; the workflow creates the release if it does not already exist.

The current release artifact target is Linux x86_64. Additional portable/static or macOS/Windows artifacts can be added later.

## Infrastructure Tokens

The deploy workflow does not provision infrastructure. These tokens are only needed for manual infrastructure changes today:

- `DIGITALOCEAN_TOKEN`
- `CLOUDFLARE_API_TOKEN`
- Backblaze B2 credentials for restic setup, if changing backup infrastructure.

## First Push Checklist

Before pushing `main`:

- Add GitHub environments: `production`, `crates-io`.
- Add GitHub secrets: `TS_OAUTH_CLIENT_ID`, `TS_OAUTH_SECRET`, `DEPLOY_SSH_KEY`.
- Confirm Tailscale ACL allows `tag:ci` to SSH to `tag:server:2222`.
- Confirm the deploy SSH public key is already present on the server through NixOS config.
- Decide the production database cutover plan for the fresh location-first schema.
- Configure crates.io Trusted Publisher for `heytea`.

After pushing:

- Confirm `CI` passes on `main`.
- Confirm `Deploy` either waits for production approval or completes successfully.
- Verify `https://api.heytea.dev/locations` and one slugged status endpoint.
