data "cloudflare_zone" "heytea" {
  name = var.cloudflare_zone_name
}

resource "digitalocean_droplet" "heytea" {
  name     = "heytea-dev"
  region   = var.digitalocean_region
  size     = var.droplet_size
  image    = var.droplet_image
  ssh_keys = [var.ssh_key_fingerprint]

  tags = ["heytea", "nixos", "production"]
}

resource "digitalocean_firewall" "heytea" {
  name = "heytea-dev"

  droplet_ids = [digitalocean_droplet.heytea.id]

  inbound_rule {
    protocol         = "tcp"
    port_range       = "80"
    source_addresses = ["0.0.0.0/0", "::/0"]
  }

  inbound_rule {
    protocol         = "tcp"
    port_range       = "443"
    source_addresses = ["0.0.0.0/0", "::/0"]
  }

  # SSH is intended to be Tailscale-only after bootstrap. Do not open 22 here.

  outbound_rule {
    protocol              = "tcp"
    port_range            = "1-65535"
    destination_addresses = ["0.0.0.0/0", "::/0"]
  }

  outbound_rule {
    protocol              = "udp"
    port_range            = "1-65535"
    destination_addresses = ["0.0.0.0/0", "::/0"]
  }
}

locals {
  public_records = toset([
    "@",
    "api",
    "docs",
    "mcp",
    "analytics",
    "status",
  ])
}

resource "cloudflare_record" "public" {
  for_each = local.public_records

  zone_id = data.cloudflare_zone.heytea.id
  name    = each.key
  type    = "A"
  value   = digitalocean_droplet.heytea.ipv4_address
  proxied = true
  ttl     = 1
}

resource "cloudflare_zone_settings_override" "heytea" {
  zone_id = data.cloudflare_zone.heytea.id

  settings {
    ssl                      = "strict"
    always_use_https         = "on"
    automatic_https_rewrites = "on"
    brotli                   = "on"
    min_tls_version          = "1.2"
  }
}

resource "b2_bucket" "backups" {
  bucket_name = var.b2_bucket_name
  bucket_type = "allPrivate"
}
