#!/usr/bin/env bash
# Provisions one admin and one non-admin access code and prints both as a single JSON object on
# stdout ({"status": "created"|"skipped", "admin": ..., "member": ...}); progress and errors go to
# stderr. Same script for dynamodb-local and real DynamoDB -- see README.md -- selected purely by
# DYNAMODB_ENDPOINT (unset or empty means real AWS, same convention as server/src/config.rs) and
# ACCESS_CODES_TABLE. No-ops if the table already has any codes, unless --force. ADMIN_CODE /
# MEMBER_CODE pin the codes instead of generating random ones, for local dev stability across
# dynamodb-local restarts.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

force=false
case "${1:-}" in
  --force) force=true ;;
  "") ;;
  *)
    echo "usage: $0 [--force]" >&2
    exit 1
    ;;
esac

for bin in aws jq openssl; do
  command -v "$bin" >/dev/null || { echo "provision-access-codes.sh: '$bin' is required but not on PATH" >&2; exit 1; }
done

table="${ACCESS_CODES_TABLE:-exalted-battle-wheel-access-codes}"

aws_args=()
[[ -n "${DYNAMODB_ENDPOINT:-}" ]] && aws_args+=(--endpoint-url "$DYNAMODB_ENDPOINT")

existing="$(aws dynamodb scan "${aws_args[@]}" --table-name "$table" --select COUNT | jq -r .Count)"
if [[ "$existing" -gt 0 && "$force" != true ]]; then
  echo "provision-access-codes.sh: $existing access code(s) already exist in $table, skipping (use --force to add another pair)" >&2
  jq -n '{status: "skipped", admin: null, member: null}'
  exit 0
fi

put_code() {
  local key="$1" is_admin="$2" label="$3"
  local now output
  now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  if ! output="$(aws dynamodb put-item "${aws_args[@]}" --table-name "$table" \
    --condition-expression "attribute_not_exists(access_key)" \
    --item "$(jq -n --arg key "$key" --arg now "$now" --argjson is_admin "$is_admin" \
      '{access_key: {S: $key}, is_admin: {BOOL: $is_admin}, created_at: {S: $now}}')" 2>&1)"; then
    if [[ "$output" == *ConditionalCheckFailedException* ]]; then
      echo "provision-access-codes.sh: $label code is pinned to a value already in $table" >&2
    else
      echo "$output" >&2
    fi
    exit 1
  fi
}

admin_code="${ADMIN_CODE:-$(openssl rand -hex 16)}"
member_code="${MEMBER_CODE:-$(openssl rand -hex 16)}"

echo "provision-access-codes.sh: creating admin code in $table..." >&2
put_code "$admin_code" true admin
echo "provision-access-codes.sh: creating non-admin code in $table..." >&2
put_code "$member_code" false member

jq -n --arg admin "$admin_code" --arg member "$member_code" '{status: "created", admin: $admin, member: $member}'
