#!/usr/bin/env bash
# Runs the whole stack locally -- dynamodb-local, the API, and the client. Ctrl-C stops all three
# and removes the DynamoDB container. See README.md.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

endpoint="http://127.0.0.1:8002"
table_json="dynamodb/access-codes-table.json"
admin_code="local-admin"

for bin in docker aws jq cargo trunk; do
  command -v "$bin" >/dev/null || { echo "dev.sh: '$bin' is required but not on PATH" >&2; exit 1; }
done
docker compose version >/dev/null || {
  echo "dev.sh: 'docker compose' plugin is required (on Arch: sudo pacman -S docker-compose)" >&2
  exit 1
}

for port in 8000 8001; do
  if ss -ltn 2>/dev/null | grep -q ":${port} "; then
    echo "dev.sh: port ${port} is already in use -- stop whatever's using it and retry" >&2
    exit 1
  fi
done

# Local-only, throwaway creds -- never the real account, even if the caller's shell already has
# AWS_PROFILE=personal exported for a deploy.
unset AWS_PROFILE
export AWS_ACCESS_KEY_ID=local AWS_SECRET_ACCESS_KEY=local AWS_REGION=us-east-1

server_pid=""
client_pid=""

cleanup() {
  trap - EXIT INT TERM
  echo
  echo "dev.sh: shutting down..."
  for pid in "$client_pid" "$server_pid"; do
    [[ -n "$pid" ]] && kill -- "-$pid" 2>/dev/null || true
  done
  for pid in "$client_pid" "$server_pid"; do
    [[ -n "$pid" ]] && wait "$pid" 2>/dev/null || true
  done
  docker compose down
}
trap cleanup EXIT
trap 'exit 130' INT TERM

docker compose up -d

echo "dev.sh: waiting for dynamodb-local..."
for _ in $(seq 1 30); do
  aws dynamodb list-tables --endpoint-url "$endpoint" >/dev/null 2>&1 && break
  sleep 1
done
aws dynamodb list-tables --endpoint-url "$endpoint" >/dev/null

table="$(jq -r .TableName "$table_json")"

# dynamodb-local runs -inMemory (see docker-compose.yml), so the table never survives a previous
# teardown -- ResourceInUseException only happens if a stale container is still running.
create_output="$(aws dynamodb create-table --endpoint-url "$endpoint" --cli-input-json "file://$table_json" 2>&1)" \
  || { [[ "$create_output" == *ResourceInUseException* ]] || { echo "$create_output" >&2; exit 1; }; }

aws dynamodb put-item --endpoint-url "$endpoint" --table-name "$table" --item "$(
  jq -n --arg key "$admin_code" --arg now "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    '{access_key: {S: $key}, is_admin: {BOOL: true}, created_at: {S: $now}}'
)" >/dev/null

set -m
ACCESS_CODES_TABLE="$table" DYNAMODB_ENDPOINT="$endpoint" cargo run -p server &
server_pid=$!
( cd client && exec trunk serve ) &
client_pid=$!

cat <<EOF

dev.sh: client   http://127.0.0.1:8000
dev.sh: server   http://127.0.0.1:8001
dev.sh: dynamodb ${endpoint}
dev.sh: admin access code: ${admin_code}

Ctrl-C to stop everything.
EOF

wait -n "$server_pid" "$client_pid"
