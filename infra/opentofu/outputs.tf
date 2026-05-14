output "droplet_ipv4" {
  value = digitalocean_droplet.heytea.ipv4_address
}

output "public_domains" {
  value = [for record in cloudflare_record.public : record.hostname]
}

output "backup_bucket" {
  value = b2_bucket.backups.bucket_name
}
