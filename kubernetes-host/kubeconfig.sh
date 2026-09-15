#!/usr/bin/env bash
# Fetches the admin kubeconfig and points it at the SSM tunnel instead of the instance's real
# address. Run ./tunnel.sh in another terminal before using the result.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
source ./_ssm.sh

wait_for_ssm
raw="$(ssm_ssh "ubuntu@$instance_id" 'sudo /usr/local/bin/k0s kubeconfig admin')"

# A dropped connection could hand back a partial file; fail loudly rather than write a broken
# kubeconfig that only breaks later.
grep -q '^[[:space:]]*server:' <<<"$raw" || {
  echo "kubeconfig fetch looks incomplete, not writing ./kubeconfig" >&2
  exit 1
}

sed -E 's|^([[:space:]]*server:).*|\1 https://127.0.0.1:6443|' <<<"$raw" >kubeconfig
chmod 600 kubeconfig
echo "wrote ./kubeconfig -- export KUBECONFIG=\$PWD/kubeconfig (needs ./tunnel.sh running)" >&2
