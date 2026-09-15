# Covers subdomains, not the apex jeffrey.lol. No AAAA: the instance has no IPv6, and Let's Encrypt
# prefers AAAA when present without falling back to A within a single validation attempt, so a
# wildcard AAAA record here would break every HTTP-01 issuance.
resource "aws_route53_record" "wildcard" {
  zone_id = local.hosted_zone_id
  name    = local.wildcard_domain
  type    = "A"
  ttl     = 60
  records = [aws_eip.this.public_ip]
}
