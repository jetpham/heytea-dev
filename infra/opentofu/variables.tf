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
  description = "NixOS image slug or custom image ID. Set during bootstrap."
  default     = "ubuntu-24-04-x64"
}

variable "ssh_key_fingerprint" {
  type        = string
  description = "DigitalOcean SSH key fingerprint used for bootstrap."
}

variable "cloudflare_zone_name" {
  type        = string
  default     = "heytea.dev"
}

variable "cloudflare_account_id" {
  type        = string
  description = "Cloudflare account ID for rulesets."
}

variable "b2_bucket_name" {
  type        = string
  default     = "heytea-dev-backups"
}
