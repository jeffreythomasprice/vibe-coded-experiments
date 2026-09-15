#!/usr/bin/env bash
# Sourced by the other scripts. Defines ssm_ssh(), which reaches the instance over an SSM session
# instead of a direct connection. No inbound SSH rule exists (or is needed) on the instance.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

instance_id="$(terraform -chdir=terraform output -raw instance_id)"

# Cloud-init no longer waits for the SSM agent's snap to finish seeding, so the instance can take a
# few minutes after boot to register. Without this wait, a fresh box reads as an SSM permissions
# failure instead of "not ready yet".
wait_for_ssm() {
  echo "waiting for SSM registration ($instance_id)..." >&2
  until aws ssm describe-instance-information \
    --filters "Key=InstanceIds,Values=$instance_id" \
    --query 'InstanceInformationList[0].PingStatus' --output text 2>/dev/null | grep -q Online; do
    sleep 5
  done
}

# -F /dev/null ignores both ~/.ssh/config and /etc/ssh/ssh_config (command-line -o flags are a
# separate, higher-precedence source and are unaffected), so this never touches global SSH state.
# The hostname passed to ssh_ssh callers is the instance id itself, so %h in ProxyCommand expands to
# it directly. accept-new matters because the box is replaced on every user_data change (see
# instance.tf) — keying known_hosts by instance id means a rebuild adds a new entry instead of
# tripping a host-key-changed warning.
ssm_ssh() {
  ssh -F /dev/null \
    -i ssh/kubernetes-host \
    -o IdentitiesOnly=yes \
    -o UserKnownHostsFile=ssh/known_hosts \
    -o StrictHostKeyChecking=accept-new \
    -o ProxyCommand="aws ssm start-session --target %h --document-name AWS-StartSSHSession --parameters portNumber=%p" \
    "$@"
}
