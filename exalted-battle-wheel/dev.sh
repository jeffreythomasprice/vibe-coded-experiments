#!/usr/bin/env bash
# Runs the whole stack locally -- dynamodb-local, the API, and the client. Ctrl-C stops all three
# and removes the DynamoDB container. See README.md.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

endpoint="http://127.0.0.1:8002"

# Optional, untracked: pins ADMIN_CODE/MEMBER_CODE across restarts instead of generating fresh
# random codes every time (dynamodb-local runs -inMemory, so the table -- and any codes in it --
# never survives one). Same "last layer wins" convention as client/build.rs's .env.local.
[[ -f .env.local ]] && set -a && . ./.env.local && set +a

# TTL attribute per table, empty for the one table (access-codes) that doesn't have one --
# `create-table --cli-input-json` can't carry TTL, so it's a separate `update-time-to-live` call
# per table, same as `terraform/dynamodb.tf`'s `ttl` block does for the real tables.
declare -A table_ttl_attribute=(
  [dynamodb/access-codes-table.json]=""
  [dynamodb/rooms-table.json]="expires_at"
  [dynamodb/websocket-connections-table.json]="expires_at"
)

for bin in docker aws jq openssl cargo trunk; do
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

access_codes_table=""
rooms_table=""
connections_table=""

for table_json in "${!table_ttl_attribute[@]}"; do
  table="$(jq -r .TableName "$table_json")"

  # dynamodb-local runs -inMemory (see docker-compose.yml), so a table never survives a previous
  # teardown -- ResourceInUseException only happens if a stale container is still running.
  create_output="$(aws dynamodb create-table --endpoint-url "$endpoint" --cli-input-json "file://$table_json" 2>&1)" \
    || { [[ "$create_output" == *ResourceInUseException* ]] || { echo "$create_output" >&2; exit 1; }; }

  ttl_attribute="${table_ttl_attribute[$table_json]}"
  if [[ -n "$ttl_attribute" ]]; then
    aws dynamodb update-time-to-live --endpoint-url "$endpoint" --table-name "$table" \
      --time-to-live-specification "Enabled=true,AttributeName=$ttl_attribute" >/dev/null
  fi

  case "$table_json" in
    dynamodb/access-codes-table.json) access_codes_table="$table" ;;
    dynamodb/rooms-table.json) rooms_table="$table" ;;
    dynamodb/websocket-connections-table.json) connections_table="$table" ;;
  esac
done

codes="$(DYNAMODB_ENDPOINT="$endpoint" ACCESS_CODES_TABLE="$access_codes_table" scripts/provision-access-codes.sh)"
admin_code="$(jq -r .admin <<<"$codes")"
member_code="$(jq -r .member <<<"$codes")"

set -m
ACCESS_CODES_TABLE="$access_codes_table" ROOMS_TABLE="$rooms_table" CONNECTIONS_TABLE="$connections_table" \
  DYNAMODB_ENDPOINT="$endpoint" cargo run -p server &
server_pid=$!
( cd client && exec trunk serve ) &
client_pid=$!

cat <<EOF

dev.sh: client   http://127.0.0.1:8000
dev.sh: server   http://127.0.0.1:8001
dev.sh: dynamodb ${endpoint}
dev.sh: admin access code:     ${admin_code}
dev.sh: non-admin access code: ${member_code}

Ctrl-C to stop everything.
EOF

wait -n "$server_pid" "$client_pid"
