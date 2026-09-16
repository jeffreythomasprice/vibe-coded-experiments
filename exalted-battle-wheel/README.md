# Exalted Battle Wheel

A Cargo workspace: `client` (the browser app), `server` (the API), `shared` (game logic and
wire types used by both).

## Run in debug mode

```
cd client && trunk serve      # client, http://127.0.0.1:8000
cargo run -p server           # server, http://127.0.0.1:8001
```

## Local testing

The server's access-code API needs DynamoDB. `docker compose up -d` runs `dynamodb-local` on
`http://127.0.0.1:8002` (requires the `docker compose` plugin: `sudo pacman -S docker-compose` on
Arch).

Create the table once:

```
export AWS_ACCESS_KEY_ID=local AWS_SECRET_ACCESS_KEY=local AWS_DEFAULT_REGION=us-east-1
aws dynamodb create-table --endpoint-url http://127.0.0.1:8002 \
  --table-name exalted-battle-wheel-access-codes \
  --attribute-definitions AttributeName=access_key,AttributeType=S \
  --key-schema AttributeName=access_key,KeyType=HASH \
  --billing-mode PAY_PER_REQUEST
```

The admin CRUD API is admin-only and the table starts empty, so the first admin code has to be
seeded directly (works the same way against the real table -- see "Deploy" below):

```
aws dynamodb put-item --endpoint-url http://127.0.0.1:8002 \
  --table-name exalted-battle-wheel-access-codes \
  --item '{"access_key":{"S":"local-admin"},"is_admin":{"BOOL":true},"created_at":{"S":"2026-01-01T00:00:00Z"}}'
```

Run the server against it (a region and credentials are required even though `dynamodb-local`
ignores them -- without them the SDK fails with "no region configured"):

```
AWS_ACCESS_KEY_ID=local AWS_SECRET_ACCESS_KEY=local AWS_REGION=us-east-1 \
DYNAMODB_ENDPOINT=http://127.0.0.1:8002 \
cargo run -p server
```

Then, with an admin token:

```
curl -H 'Authorization: Bearer local-admin' localhost:8001/auth/me
curl -H 'Authorization: Bearer local-admin' -X POST -H 'Content-Type: application/json' \
  -d '{"is_admin":false}' localhost:8001/access-codes
curl -H 'Authorization: Bearer local-admin' localhost:8001/access-codes
curl -H 'Authorization: Bearer local-admin' -X PUT -H 'Content-Type: application/json' \
  -d '{"is_admin":true}' localhost:8001/access-codes/<access_key>
curl -H 'Authorization: Bearer local-admin' -X DELETE localhost:8001/access-codes/<access_key>
```

A request with no `Authorization` header, or an unrecognized code, gets `401`; a non-admin code
against any `/access-codes` route gets `403`.

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

The access-codes table starts empty; seed the first admin code once, the same way as in "Local
testing" but against the real table (no `--endpoint-url`):

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
