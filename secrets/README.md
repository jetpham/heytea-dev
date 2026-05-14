# Secrets

Encrypted agenix secrets live here. Do not commit plaintext secret files.

Create or edit secrets from this directory with:

```sh
cd secrets
agenix -e tailscale-auth-key.age
agenix -e grafana-secret-key.age
agenix -e grafana-admin-password.age
```

Generate random service secrets without leaving plaintext in the repo:

```sh
cd secrets
openssl rand -hex 64 | RULES=./secrets.nix agenix -e grafana-secret-key.age
openssl rand -base64 48 | RULES=./secrets.nix agenix -e grafana-admin-password.age
```

Create the Tailscale auth key secret by pasting the fresh key into a hidden prompt:

```sh
cd secrets
read -rsp "Tailscale auth key: " TS_KEY; printf '\n'; printf '%s\n' "$TS_KEY" | RULES=./secrets.nix agenix -e tailscale-auth-key.age; unset TS_KEY
```

Before production deploy, add the server recipient key to `secrets/secrets.nix`, then rekey the secrets.

The first server boot can still join Tailscale manually. Once `tailscale-auth-key.age` exists and decrypts on the server, NixOS will run `tailscale up --hostname=heytea-dev --advertise-tags=tag:server` automatically when needed.

Grafana stays disabled until `grafana-secret-key.age` and `grafana-admin-password.age` exist, because NixOS 26.05 requires an explicit stable Grafana secret key and production should not use the default admin password.
