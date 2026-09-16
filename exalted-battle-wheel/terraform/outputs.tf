output "distribution_id" {
  value = aws_cloudfront_distribution.site.id
}

output "distribution_domain_name" {
  value = aws_cloudfront_distribution.site.domain_name
}

output "site_url" {
  value = "https://${local.domain}"
}

output "access_codes_table" {
  value = aws_dynamodb_table.this["access_codes"].name
}

output "rooms_table" {
  value = aws_dynamodb_table.this["rooms"].name
}

output "connections_table" {
  value = aws_dynamodb_table.this["connections"].name
}

output "server_access_key_id" {
  value = aws_iam_access_key.server.id
}

output "server_secret_access_key" {
  value     = aws_iam_access_key.server.secret
  sensitive = true
}
