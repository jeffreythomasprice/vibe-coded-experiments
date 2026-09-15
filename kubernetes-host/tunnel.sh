#!/usr/bin/env bash
# Forwards local :6443 to the API server over SSM. Runs in the foreground; Ctrl-C to stop.
# Uses `ssh -L` rather than the AWS-StartPortForwardingSession document — older Session Manager
# plugins serialize concurrent connections through one port-forwarding session, which kubectl (which
# opens several: watches, logs -f, etc.) trips over. SSH multiplexes channels properly over the single
# underlying SSM stream.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
source ./_ssm.sh

wait_for_ssm
echo "tunneling localhost:6443 -> instance:6443 (Ctrl-C to stop)" >&2
ssm_ssh -N -L 6443:127.0.0.1:6443 "ubuntu@$instance_id"
