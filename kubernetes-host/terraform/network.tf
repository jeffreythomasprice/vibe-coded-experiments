# Terraform removes AWS's default allow-all egress rule when it creates a security group, so an
# explicit egress rule is required — without it the instance has no outbound access at all (no SSM
# registration, no k0s download, no image pulls, no ACME) and is permanently unreachable.

resource "aws_security_group" "this" {
  name        = "kubernetes-host"
  description = "k0s host: public HTTP/HTTPS in, all out. Admin access is via SSM, no inbound needed."
  vpc_id      = local.vpc_id
}

resource "aws_vpc_security_group_ingress_rule" "http" {
  security_group_id = aws_security_group.this.id
  description       = "HTTP for ingress-nginx and ACME HTTP-01"
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "tcp"
  from_port         = 80
  to_port           = 80
}

resource "aws_vpc_security_group_ingress_rule" "https" {
  security_group_id = aws_security_group.this.id
  description       = "HTTPS for ingress-nginx"
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "tcp"
  from_port         = 443
  to_port           = 443
}

resource "aws_vpc_security_group_egress_rule" "all" {
  security_group_id = aws_security_group.this.id
  description       = "All outbound: SSM agent, k0s/helm chart downloads, image pulls, ACME"
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
}
