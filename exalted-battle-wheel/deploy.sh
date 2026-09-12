#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

bucket="jeff-exalted-battle-wheel"
distribution_id="$(terraform -chdir=terraform output -raw distribution_id)"

trunk build --release

aws s3 sync dist/ "s3://${bucket}/" \
  --delete \
  --exclude index.html \
  --cache-control "public,max-age=31536000,immutable"

for wasm in dist/*.wasm; do
  aws s3 cp "$wasm" "s3://${bucket}/$(basename "$wasm")" \
    --content-type application/wasm \
    --cache-control "public,max-age=31536000,immutable"
done

aws s3 cp dist/index.html "s3://${bucket}/index.html" \
  --content-type "text/html; charset=utf-8" \
  --cache-control "no-cache"

aws cloudfront create-invalidation \
  --distribution-id "$distribution_id" \
  --paths '/*'
