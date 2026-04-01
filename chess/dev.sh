#!/usr/bin/env bash
set -euo pipefail

SERVER_PID=""
CLIENT_PID=""

cleanup() {
    if [[ -n "$SERVER_PID" ]]; then
        kill "$SERVER_PID" 2>/dev/null
        SERVER_PID=""
    fi
    if [[ -n "$CLIENT_PID" ]]; then
        kill "$CLIENT_PID" 2>/dev/null
        CLIENT_PID=""
    fi
    wait 2>/dev/null
}
trap cleanup EXIT INT TERM

docker compose up -d

watchexec -r 'cargo run -p chess-server' &
SERVER_PID=$!

pushd client
trunk serve &
CLIENT_PID=$!
popd

wait
