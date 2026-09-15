#!/usr/bin/env bash
# Deploys the client, the server, or both (default). See CLAUDE.md for how each half is hosted,
# how certs work, and what a redeploy actually does.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

target="${1:-all}"

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

  # A fresh tag every deploy, never reused: there's no registry, so the tag in the manifest is how
  # the node tells old bits from new ones (see CLAUDE.md).
  local tag="v$(date -u +%Y%m%d%H%M%S)"
  local image="exalted-server:${tag}"

  docker buildx build --platform linux/arm64 -f server/Dockerfile -t "$image" --load .
  (cd ../kubernetes-host && ./push-image.sh "$image")

  sed "s|exalted-server:[^ ]*|${image}|" server/manifest.yaml | kubectl apply -f -
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
  *)
    echo "usage: $0 [client|server]  -- deploys both if omitted" >&2
    exit 1
    ;;
esac
