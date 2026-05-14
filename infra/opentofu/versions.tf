terraform {
  required_version = ">= 1.8.0"

  required_providers {
    digitalocean = {
      source  = "digitalocean/digitalocean"
      version = "~> 2.47"
    }
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 4.49"
    }
    b2 = {
      source  = "Backblaze/b2"
      version = "~> 0.10"
    }
  }
}

provider "digitalocean" {}
provider "cloudflare" {}
provider "b2" {}
