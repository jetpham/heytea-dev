# NixOS DigitalOcean Deploy

This is the preferred first-production path for `heytea.dev`: build a minimal NixOS DigitalOcean bootstrap image, upload it to DigitalOcean, create the `heytea-dev` droplet with OpenTofu, then use deploy-rs for full production updates.

## Build The Image

```sh
nix build .#nixos-do-image
```

The result is a DigitalOcean-compatible NixOS image from `nixosConfigurations.heytea-bootstrap`. It contains only enough NixOS, SSH, and Tailscale support to make the droplet reachable for the first deploy.

The upload artifact is:

```text
result/heytea-dev-digital-ocean.qcow2
```

## Upload The Image

DigitalOcean custom images accept Linux images such as qcow2 or raw images. Upload the built qcow2 image to the `sfo3` region with the control panel or `doctl`.

Example shape:

```sh
doctl compute image create heytea-nixos \
  --image-url "https://example.com/heytea-nixos-image.qcow2" \
  --region sfo3
```

If you upload through the web UI, copy the resulting custom image ID. Do not commit that ID unless you intentionally want it as shared infrastructure state.

## Provision Infrastructure

Create a local, ignored `infra/opentofu/tofu.tfvars`:

```hcl
droplet_image        = "<digitalocean-custom-image-id>"
ssh_key_fingerprint = "<digitalocean-ssh-key-fingerprint>"
cloudflare_account_id = "<cloudflare-account-id>"
bootstrap_ssh_source_addresses = ["<your-current-public-ip>/32"]
```

Then validate and apply only when ready:

```sh
cd infra/opentofu
tofu init
tofu plan
tofu apply
```

## Bootstrap Tailscale

The previously pasted Tailscale auth key must be revoked. Create a fresh auth key with these properties:

- preauthorized
- single-use
- short expiry
- non-ephemeral for this production server

After the droplet boots, SSH in with the configured public key and run:

```sh
ssh root@<droplet-ip>
tailscale up --auth-key=<fresh-key> --hostname=heytea-dev --advertise-tags=tag:server
```

After the node appears in Tailscale, remove `bootstrap_ssh_source_addresses` from `tofu.tfvars` and apply OpenTofu again to close public SSH:

```sh
tofu apply
```

Administrative SSH should go over Tailscale after bootstrap:

```sh
ssh root@heytea-dev
```

## Secrets

Do not bake secrets into the image, OpenTofu state, or Git.

The image contains service definitions and public configuration only. Runtime secrets should be added through agenix before production use. Required secrets currently include:

- Grafana secret key at `/run/agenix/grafana-secret-key`
- Future backup credentials for Backblaze B2/restic
- Future analytics secrets, if Umami is enabled
- Optional Tailscale bootstrap key at `/run/agenix/tailscale-auth-key`, if manual bootstrap is replaced

For the first boot, Tailscale can be bootstrapped manually with a fresh one-time key. After that, use deploy-rs over the tailnet.

## Deploy Updates

Once the droplet is reachable as `heytea-dev` on Tailscale:

```sh
deploy .#heytea-dev
```

The deploy target is defined in `flake.nix` and activates `nixosConfigurations.heytea-dev` as root.

## Verify

After activation:

```sh
curl https://api.heytea.dev/readyz
curl https://api.heytea.dev/status
curl -N https://api.heytea.dev/stream
curl https://heytea.dev/llms.txt
```

Then run an AgentGrade scan once DNS and Cloudflare proxying are live.
