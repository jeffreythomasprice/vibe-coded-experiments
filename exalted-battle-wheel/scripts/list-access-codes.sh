#!/usr/bin/env bash
# Prints every access code and its metadata, concisely, newest first. Same script for
# dynamodb-local and real DynamoDB -- see README.md -- selected purely by DYNAMODB_ENDPOINT (unset
# or empty means real AWS, same convention as server/src/config.rs) and ACCESS_CODES_TABLE.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

for bin in aws jq column; do
  command -v "$bin" >/dev/null || { echo "list-access-codes.sh: '$bin' is required but not on PATH" >&2; exit 1; }
done

table="${ACCESS_CODES_TABLE:-exalted-battle-wheel-access-codes}"

aws_args=()
[[ -n "${DYNAMODB_ENDPOINT:-}" ]] && aws_args+=(--endpoint-url "$DYNAMODB_ENDPOINT")

scan_output="$(aws dynamodb scan "${aws_args[@]}" --table-name "$table")"

if [[ "$(jq '.Items | length' <<<"$scan_output")" -eq 0 ]]; then
  echo "list-access-codes.sh: no access codes in $table" >&2
  exit 0
fi

{
  echo -e "ACCESS_KEY\tROLE\tCREATED_AT"
  jq -r '.Items | sort_by(.created_at.S) | reverse | .[] |
    [.access_key.S, (if .is_admin.BOOL then "admin" else "member" end), .created_at.S] | @tsv' <<<"$scan_output"
} | column -t -s $'\t'
