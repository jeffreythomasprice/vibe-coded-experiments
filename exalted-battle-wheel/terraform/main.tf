terraform {
  required_version = ">= 1.11"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
    random = {
      source  = "hashicorp/random"
      version = "~> 3.0"
    }
  }

  backend "s3" {
    bucket       = "jeffs-tfstate"
    key          = "exalted-battle-wheel/terraform.tfstate"
    region       = "us-east-1"
    use_lockfile = true
    encrypt      = true
  }
}

provider "aws" {
  region = "us-east-1"
}

locals {
  domain         = "exalted.jeffrey.lol"
  bucket         = "jeff-exalted-battle-wheel"
  cert_domain    = "*.jeffrey.lol"
  hosted_zone_id = "Z234AULK6UMOJT"
  origin_id      = "s3-site"
}
