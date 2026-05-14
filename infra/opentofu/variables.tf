variable "digitalocean_region" {
  type        = string
  description = "DigitalOcean region closest to the restaurant."
  default     = "sfo3"
}

variable "droplet_size" {
  type        = string
  description = "Initial VPS size."
  default     = "s-2vcpu-4gb"
}

variable "droplet_image" {
  type        = string
  description = "DigitalOcean custom image ID for the NixOS image built with `nix build .#nixos-do-image`."
}

variable "ssh_key_fingerprint" {
  type        = string
  description = "DigitalOcean SSH key fingerprint used for bootstrap."
}

variable "bootstrap_ssh_source_addresses" {
  type        = list(string)
  description = "Temporary CIDRs allowed to SSH during first boot before Tailscale is joined. Leave empty after bootstrap."
  default     = []
}

variable "cloudflare_zone_name" {
  type    = string
  default = "heytea.dev"
}

variable "cloudflare_account_id" {
  type        = string
  description = "Cloudflare account ID for rulesets."
}

variable "b2_bucket_name" {
  type    = string
  default = "heytea-dev-backups"
}
