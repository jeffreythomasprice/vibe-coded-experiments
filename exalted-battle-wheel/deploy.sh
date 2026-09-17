#!/usr/bin/env bash
# Deploys the client, the server, or both (default). See CLAUDE.md for how each half is hosted,
# how certs work, and what a redeploy actually does.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

export AWS_PROFILE="${AWS_PROFILE:-personal}"
export AWS_REGION="${AWS_REGION:-us-east-1}"

target="${1:-all}"
case "$target" in
  all | client | server) ;;
  *)
    echo "usage: $0 [client|server]  -- deploys both if omitted" >&2
    exit 1
    ;;
esac

# Applies whatever's in terraform/ before every deploy, so a config change (a new output, a new
# table, ...) never needs a separately-remembered `terraform apply` first -- deploy_server below
# reads outputs that don't exist in state until apply has run at least once (see CLAUDE.md's
# "Server hosting" section). A no-op plan just prints "No changes" and returns without prompting.
terraform -chdir=terraform init
terraform -chdir=terraform apply

deploy_client() {
  local bucket="jeff-exalted-battle-wheel"
  local distribution_id
  distribution_id="$(terraform -chdir=terraform output -raw distribution_id)"

  (cd client && trunk build --release)

  aws s3 sync client/dist/ "s3://${bucket}/" \
    --delete \
    --exclude index.html \
    --cache-control "public,max-age=31536000,immutable"

  for wasm in client/dist/*.wasm; do
    aws s3 cp "$wasm" "s3://${bucket}/$(basename "$wasm")" \
      --content-type application/wasm \
      --cache-control "public,max-age=31536000,immutable"
  done

  aws s3 cp client/dist/index.html "s3://${bucket}/index.html" \
    --content-type "text/html; charset=utf-8" \
    --cache-control "no-cache"

  aws cloudfront create-invalidation \
    --distribution-id "$distribution_id" \
    --paths '/*'
}

deploy_server() {
  : "${KUBECONFIG:?KUBECONFIG must point at ../kubernetes-host/kubeconfig -- export it and run ../kubernetes-host/tunnel.sh in another terminal first}"

  # Read every output into a variable first rather than inline in the kubectl calls below -- inline
  # `--from-literal=X="$(terraform output ...)"` swallows a terraform failure (kubectl's own exit
  # status is what `set -e` sees, not the substitution's), silently pushing empty secrets instead
  # of aborting.
  local access_key_id secret_access_key session_secret access_codes_table
  access_key_id="$(terraform -chdir=terraform output -raw server_access_key_id)"
  secret_access_key="$(terraform -chdir=terraform output -raw server_secret_access_key)"
  session_secret="$(terraform -chdir=terraform output -raw session_secret)"
  access_codes_table="$(terraform -chdir=terraform output -raw access_codes_table)"

  # The pod can't use the node's instance role (IMDS hop limit 1, deliberately -- see CLAUDE.md), so
  # its DynamoDB credentials come from a dedicated IAM user's access key instead, upserted into a
  # Secret from the current Terraform outputs on every deploy.
  kubectl create secret generic exalted-server-aws \
    --from-literal=AWS_ACCESS_KEY_ID="$access_key_id" \
    --from-literal=AWS_SECRET_ACCESS_KEY="$secret_access_key" \
    --dry-run=client -o yaml | kubectl apply -f -

  # Signs room session tokens (see server/src/sessions.rs) -- a stable value held in Terraform
  # state, not generated in-process, so a redeploy or pod restart never invalidates every
  # outstanding session (see terraform/sessions.tf and CLAUDE.md's "Server hosting" section).
  kubectl create secret generic exalted-server-session \
    --from-literal=SESSION_SECRET="$session_secret" \
    --dry-run=client -o yaml | kubectl apply -f -

  # Cross-building for arm64 (see CLAUDE.md) needs a QEMU binfmt handler registered on this host's
  # kernel; it doesn't survive a reboot, so re-register whenever it's missing rather than assuming
  # a one-time setup step.
  if [[ ! -e /proc/sys/fs/binfmt_misc/qemu-aarch64 ]]; then
    echo "deploy.sh: registering QEMU binfmt handler for linux/arm64 emulation..." >&2
    docker run --privileged --rm tonistiigi/binfmt --install arm64
  fi

  # A fresh tag every deploy, never reused: there's no registry, so the tag in the manifest is how
  # the node tells old bits from new ones (see CLAUDE.md).
  local tag="v$(date -u +%Y%m%d%H%M%S)"
  local image="exalted-server:${tag}"

  docker buildx build --platform linux/arm64 -f server/Dockerfile -t "$image" --load .
  (cd ../kubernetes-host && ./push-image.sh "$image")

  sed "s|exalted-server:[^ ]*|${image}|" server/manifest.yaml | kubectl apply -f -

  # No-ops once an admin + non-admin pair already exist (every deploy after the first) --
  # DYNAMODB_ENDPOINT is set empty explicitly so an exported local value can't redirect this at
  # dynamodb-local, same convention as server/src/config.rs treating empty as unset.
  local provision_output
  provision_output="$(DYNAMODB_ENDPOINT= ACCESS_CODES_TABLE="$access_codes_table" scripts/provision-access-codes.sh)"
  if [[ "$(jq -r .status <<<"$provision_output")" == created ]]; then
    echo "deploy.sh: admin access code:     $(jq -r .admin <<<"$provision_output")"
    echo "deploy.sh: non-admin access code: $(jq -r .member <<<"$provision_output")"
  fi
}

case "$target" in
  all)
    deploy_client
    deploy_server
    ;;
  client)
    deploy_client
    ;;
  server)
    deploy_server
    ;;
esac
