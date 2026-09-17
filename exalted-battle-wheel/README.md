# Exalted Battle Wheel

A Cargo workspace: `client` (the browser app), `server` (the API), `shared` (game logic and
wire types used by both). Every wire type is generated from JSON Schema at build time; see
`shared/schemas/README.md`.

## Run locally

```
./dev.sh
```

Starts `dynamodb-local` (8002), the API (8001), and the client at http://127.0.0.1:8000; Ctrl-C
stops all three and removes the DynamoDB container. It creates all three tables (access codes,
rooms, websocket connections) from the JSON files under `dynamodb/` -- the same files
`terraform/dynamodb.tf` builds the real tables from -- and provisions an admin and a non-admin
access code via `scripts/provision-access-codes.sh`, printing both. Everything is recreated on
every run, since `dynamodb-local` runs in-memory; to keep the same codes across restarts, set
`ADMIN_CODE`/`MEMBER_CODE` in an untracked `.env.local`.

Needs `docker compose` (`sudo pacman -S docker-compose` on Arch), plus `aws`, `jq`, `openssl`, and
`trunk`.

With the stack up, and the admin token `./dev.sh` printed:

```
curl -H "Authorization: Bearer $ADMIN_CODE" localhost:8001/auth/me
curl -H "Authorization: Bearer $ADMIN_CODE" -X POST -H 'Content-Type: application/json' \
  -d '{"is_admin":false}' localhost:8001/access-codes
curl -H "Authorization: Bearer $ADMIN_CODE" localhost:8001/access-codes
curl -H "Authorization: Bearer $ADMIN_CODE" -X PUT -H 'Content-Type: application/json' \
  -d '{"is_admin":true}' localhost:8001/access-codes/<access_key>
curl -H "Authorization: Bearer $ADMIN_CODE" -X DELETE localhost:8001/access-codes/<access_key>
curl -H "Authorization: Bearer $ADMIN_CODE" "localhost:8001/rooms?q=&limit=20&cursor="
curl -H "Authorization: Bearer $ADMIN_CODE" -X DELETE "localhost:8001/rooms/<room name>"
```

A request with no `Authorization` header, or an unrecognized code, gets `401`; a non-admin code
against any `/access-codes` or `/rooms` route gets `403`. `GET /rooms` and `DELETE /rooms/<room
name>` are both admin-only, for the in-app "All rooms" browser (`q`/`limit`/`cursor` are all
optional). Any access code -- admin or not -- may create, join, and play in a room over the
websocket at `/ws`; see `MULTIPLAYER.md`.

## Test

```
cargo test --workspace
```

## Build for release

```
cd client && trunk build --release
```

## Deploy

One-time infrastructure setup (state bucket `jeffs-tfstate` must already exist):

```
export AWS_PROFILE=personal
terraform -chdir=terraform init
terraform -chdir=terraform apply
```

For the server half, `../kubernetes-host` must already be stood up, and its kubectl tunnel running
in another terminal (`./kubeconfig.sh` once, then `./tunnel.sh`) with `KUBECONFIG` exported to point
at it.

Every subsequent deploy:

```
export AWS_PROFILE=personal
export AWS_REGION=us-east-1
export KUBECONFIG=../kubernetes-host/kubeconfig
./deploy.sh          # both client and server
./deploy.sh client   # just the client
./deploy.sh server   # just the server
```

The access-codes table starts empty; `./deploy.sh server` provisions an admin and a non-admin code
on its first run (via `scripts/provision-access-codes.sh`, the same script `dev.sh` uses locally)
and prints both -- later deploys leave existing codes alone. To provision or review codes without a
full deploy:

```
export AWS_PROFILE=personal
ACCESS_CODES_TABLE="$(terraform -chdir=terraform output -raw access_codes_table)" \
  scripts/provision-access-codes.sh
ACCESS_CODES_TABLE="$(terraform -chdir=terraform output -raw access_codes_table)" \
  scripts/list-access-codes.sh
```

Then, against the deployed API:

```
curl -H "Authorization: Bearer $ADMIN_CODE" https://exalted-api.jeffrey.lol/auth/me
curl -H "Authorization: Bearer $ADMIN_CODE" https://exalted-api.jeffrey.lol/access-codes
```

See `CLAUDE.md` for how each half is hosted and what a redeploy does, and `MULTIPLAYER.md` for
connecting and testing rooms.
