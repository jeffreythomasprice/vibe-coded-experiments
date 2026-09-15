TODO.md is for humans. You can read it for context only if the user explicitly mentions it, and you can never update it.

REAMDE.md should be kept fairly terse. This shouldn't be explaining project architecture, just how to build and run it.

# Hosting

A single `t4g.small` EC2 instance runs single-node k0s (Ubuntu 24.04 arm64), reachable at any
`*.jeffrey.lol` subdomain via a wildcard Route53 record pointed at its Elastic IP. TLS is Let's Encrypt
via cert-manager (HTTP-01), not ACM -- ACM certs can't attach to a plain EC2 instance, and the existing
`*.jeffrey.lol` ACM cert belongs to `exalted-battle-wheel`'s state anyway. There is no container
registry: images are built locally, `docker save`d, and streamed straight into the node's containerd.

Admin access -- kubectl, the kubeconfig fetch, and image pushes -- goes entirely over SSH tunneled
through an SSM session (`AWS-StartSSHSession`). The security group has **no inbound 22 or 6443**: both
run on the instance's own loopback and the SSM agent dials out, so no inbound rule is needed or
present. `_ssm.sh` defines the shared SSH invocation; it passes `-F /dev/null` plus explicit `-o` flags
so nothing here ever touches `~/.ssh/config`.

The instance is cattle, not a pet: `user_data_replace_on_change = true` means any edit to the k0s
config, the bootstrap script, or the ClusterIssuers destroys and recreates the instance (and everything
running on it) on the next `terraform apply`. This is deliberate -- `user_data` changes default to a
silent stop/start otherwise, and cloud-init's `runcmd` doesn't rerun on an unchanged instance id, so an
edit would otherwise appear to succeed while doing nothing.

A `t4g.small` has 2GB RAM, which the k0s control plane, kube-router, coredns, ingress-nginx, and
cert-manager consume most of by themselves. `terraform/k0s.yaml.tftpl` sets `workerProfiles[name=default]`
resource reservations so the scheduler doesn't believe the ~1GB used by non-pod control-plane processes
is available to pods, plus a swapfile and an `OOMScoreAdjust` drop-in so the kernel OOM killer takes a
pod instead of kube-apiserver under pressure. Don't remove these without re-deriving the memory budget.

Terraform state lives in `s3://jeffs-tfstate/kubernetes-host/terraform.tfstate`, using S3's native
locking (`use_lockfile`, no DynamoDB table). That state bucket was created by hand and is shared with
other projects.

`terraform apply`/`destroy` and the `*.sh` scripts are human-only actions. Claude may write and edit the
Terraform config, the k0s config, and the scripts, but must never run anything that touches AWS or
Terraform state -- no `terraform init`/`plan`/`apply`/`destroy`, no `./*.sh`, and no mutating `aws`
commands. Offline validation is fine and encouraged: `terraform fmt`, `terraform init -backend=false`,
and `terraform validate`. Report what a change would do and let the user run it. Working AWS credentials
are in the `personal` CLI profile, not the default one.