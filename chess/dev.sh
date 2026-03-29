#!/usr/bin/env bash
set -euo pipefail

# Recursively kill a process and all its descendants
killtree() {
    local pid=$1
    # Kill children first (depth-first)
    for child in $(pgrep -P "$pid" 2>/dev/null); do
        killtree "$child"
    done
    kill "$pid" 2>/dev/null
}

PIDS=()

cleanup() {
    for pid in "${PIDS[@]}"; do
        killtree "$pid"
    done
    wait 2>/dev/null
}
trap cleanup EXIT INT TERM

docker compose up -d

cargo watch -x 'run -p chess-server' &
PIDS+=($!)

(cd client && trunk serve) &
PIDS+=($!)

wait
