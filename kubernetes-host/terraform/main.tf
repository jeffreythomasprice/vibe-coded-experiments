terraform {
  required_version = ">= 1.11"
  required_providers {
    aws = { source = "hashicorp/aws", version = "~> 6.0" }
  }
  backend "s3" {
    bucket       = "jeffs-tfstate"
    key          = "kubernetes-host/terraform.tfstate"
    region       = "us-east-1"
    use_lockfile = true
    encrypt      = true
  }
}

provider "aws" { region = "us-east-1" }

locals {
  hosted_zone_id  = "Z234AULK6UMOJT"
  wildcard_domain = "*.jeffrey.lol"
  instance_type   = "t4g.small"
  vpc_id          = "vpc-a63215c3"

  k0s_version           = "v1.36.4+k0s.0"
  cert_manager_version  = "v1.21.2"
  ingress_nginx_version = "4.14.5"

  # Sent to Let's Encrypt for account registration; optional, can be set to "".
  acme_email = "jeffrey.thomas.price@gmail.com"
}

data "aws_ssm_parameter" "ubuntu_ami" {
  name = "/aws/service/canonical/ubuntu/server/24.04/stable/current/arm64/hvm/ebs-gp3/ami-id"
}
