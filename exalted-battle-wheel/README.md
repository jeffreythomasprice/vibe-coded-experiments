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
`terraform/dynamodb.tf` builds the real tables from -- and seeds an admin code `local-admin`.
Everything is recreated on every run, since `dynamodb-local` runs in-memory.

Needs `docker compose` (`sudo pacman -S docker-compose` on Arch), plus `aws`, `jq`, and `trunk`.

With the stack up, and an admin token:

```
curl -H 'Authorization: Bearer local-admin' localhost:8001/auth/me
curl -H 'Authorization: Bearer local-admin' -X POST -H 'Content-Type: application/json' \
  -d '{"is_admin":false}' localhost:8001/access-codes
curl -H 'Authorization: Bearer local-admin' localhost:8001/access-codes
curl -H 'Authorization: Bearer local-admin' -X PUT -H 'Content-Type: application/json' \
  -d '{"is_admin":true}' localhost:8001/access-codes/<access_key>
curl -H 'Authorization: Bearer local-admin' -X DELETE localhost:8001/access-codes/<access_key>
curl -H 'Authorization: Bearer local-admin' localhost:8001/rooms
```

A request with no `Authorization` header, or an unrecognized code, gets `401`; a non-admin code
against any `/access-codes` route gets `403`. `/rooms` needs a valid code but not an admin one --
any access code may create, join, and play in a room, over the websocket at `/ws`; see
`MULTIPLAYER.md`.

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

The access-codes table starts empty; seed the first admin code once, the same shape `dev.sh` seeds
locally but against the real table (no `--endpoint-url`):

```
export AWS_PROFILE=personal
KEY="$(openssl rand -hex 16)" && echo "$KEY"
aws dynamodb put-item --table-name exalted-battle-wheel-access-codes \
  --item "{\"access_key\":{\"S\":\"$KEY\"},\"is_admin\":{\"BOOL\":true},\"created_at\":{\"S\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}}"
```

Then, against the deployed API:

```
curl -H 'Authorization: Bearer <access_key>' https://exalted-api.jeffrey.lol/auth/me
curl -H 'Authorization: Bearer <access_key>' https://exalted-api.jeffrey.lol/access-codes
```

See `CLAUDE.md` for how each half is hosted and what a redeploy does, and `MULTIPLAYER.md` for
connecting and testing rooms.
