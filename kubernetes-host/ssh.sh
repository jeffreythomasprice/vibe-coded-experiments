#!/usr/bin/env bash
# Shell (or one-shot command) on the instance, over SSM. Usage: ./ssh.sh [command...]
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
source ./_ssm.sh

wait_for_ssm
ssm_ssh "ubuntu@$instance_id" "$@"
