data "aws_subnets" "default" {
  filter {
    name   = "vpc-id"
    values = [local.vpc_id]
  }
  filter {
    name   = "default-for-az"
    values = ["true"]
  }
}

# Standalone EIP, associated separately below. Deliberately not `aws_instance.this.public_ip = ...` —
# that would create instance -> eip -> instance-association-back cycle. This way `public_ip` is known
# at plan time and the bootstrap script/k0s config can reference it.
resource "aws_eip" "this" {
  domain = "vpc"
}

locals {
  k0s_yaml = templatefile("${path.module}/k0s.yaml.tftpl", {
    cert_manager_version  = local.cert_manager_version
    ingress_nginx_version = local.ingress_nginx_version
  })

  cluster_issuers = templatefile("${path.module}/clusterissuers.yaml.tftpl", {
    acme_email = local.acme_email
  })

  bootstrap = templatefile("${path.module}/bootstrap.sh.tftpl", {
    k0s_version = local.k0s_version
    eip         = aws_eip.this.public_ip
  })

  # yamlencode() rather than a hand-written cloud-config template: the payload nests a multi-line k0s
  # config (which itself nests multi-line inline Helm values) inside a write_files content block inside
  # YAML, and hand-indenting three levels of that is exactly where a silently-dropped key comes from.
  cloud_config = "#cloud-config\n${yamlencode({
    swap = {
      filename = "/swapfile"
      size     = 2147483648
      maxsize  = 2147483648
    }
    # Top-level list, not a `users:` block — a `users:` list without `- default` first deletes the
    # `ubuntu` user and locks out ssh.sh/tunnel.sh/push-image.sh.
    ssh_authorized_keys = [trimspace(file("${path.module}/../ssh/kubernetes-host.pub"))]
    write_files = [
      {
        path        = "/etc/k0s/k0s.yaml"
        permissions = "0600"
        content     = local.k0s_yaml
      },
      {
        # Deliberately NOT under /var/lib/k0s/manifests/ — k0s's manifest-stack applier only retries a
        # fixed ~10 times over about a minute after a file change, then gives up for good. cert-manager's
        # webhook routinely isn't accepting connections yet within that window on a memory-constrained
        # node, so anything dropped there never gets created. bootstrap.sh applies this file directly
        # instead, retrying until cert-manager is actually ready.
        path    = "/etc/k0s/clusterissuers.yaml"
        content = local.cluster_issuers
      },
      {
        path        = "/usr/local/sbin/bootstrap.sh"
        permissions = "0755"
        content     = local.bootstrap
      },
    ]
    runcmd = [["/usr/local/sbin/bootstrap.sh"]]
  })}"
}

resource "aws_instance" "this" {
  ami                    = data.aws_ssm_parameter.ubuntu_ami.value
  instance_type          = local.instance_type
  subnet_id              = sort(data.aws_subnets.default.ids)[0]
  vpc_security_group_ids = [aws_security_group.this.id]
  iam_instance_profile   = aws_iam_instance_profile.this.name

  associate_public_ip_address = true

  root_block_device {
    volume_size = 30    # 8GB default is not enough: k0s ~2GB + Ubuntu ~2.5GB + image store + swap,
    volume_type = "gp3" # and kubelet starts evicting once free space drops below 15%.
    encrypted   = true
  }

  metadata_options {
    http_endpoint = "enabled"
    http_tokens   = "required"
    # Deliberately 1, not the usual EKS-recommended 2: nothing here needs IMDS from a pod (no
    # cloud-controller-manager, no CSI driver, no external-dns), and hop limit 2 would let any pod
    # read the instance role's credentials. At 1, pod traffic (crossing the kube-bridge veth) is
    # blocked while host processes and host-network pods are unaffected.
    http_put_response_hop_limit = 1
    instance_metadata_tags      = "disabled"
  }

  credit_specification {
    cpu_credits = "standard" # caps cost; flip to "unlimited" if the box gets throttled under load
  }

  # user_data changes default to a stop/start, and cloud-init's runcmd is once-per-instance-id — so
  # editing the k0s config would otherwise silently do nothing while `apply` reports success.
  # This box is meant to be cattle: replacement is the actual (and faster) update path.
  user_data_base64            = base64gzip(local.cloud_config)
  user_data_replace_on_change = true

  # No create_before_destroy: two instances can't share one aws_eip_association, so that would strand
  # a partial apply. Rebuilds go through the ordinary destroy-then-create path instead.
}

resource "aws_eip_association" "this" {
  instance_id   = aws_instance.this.id
  allocation_id = aws_eip.this.id
}
