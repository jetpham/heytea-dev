# Secrets

Encrypted agenix secrets live here. Do not commit plaintext secret files.

Create or edit secrets from this directory with:

```sh
cd secrets
agenix -e tailscale-auth-key.age
```

Create the Tailscale auth key secret by pasting the fresh key into a hidden prompt:

```sh
cd secrets
read -rsp "Tailscale auth key: " TS_KEY; printf '\n'; printf '%s\n' "$TS_KEY" | RULES=./secrets.nix agenix -e tailscale-auth-key.age; unset TS_KEY
```

Before production deploy, add the server recipient key to `secrets/secrets.nix`, then rekey the secrets.

The first server boot can still join Tailscale manually. Once `tailscale-auth-key.age` exists and decrypts on the server, NixOS will run `tailscale up --hostname=heytea-dev --advertise-tags=tag:server` automatically when needed. Cloudflare API credentials are GitHub Actions secrets, not agenix runtime secrets.
