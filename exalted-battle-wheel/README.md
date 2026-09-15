# Exalted Battle Wheel

A Cargo workspace: `client` (the browser app), `server` (the API), `shared` (game logic and
wire types used by both).

## Run in debug mode

```
cd client && trunk serve      # client, http://127.0.0.1:8000
cargo run -p server           # server, http://127.0.0.1:8001
```

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

See `CLAUDE.md` for how each half is hosted and what a redeploy does, and `MULTIPLAYER.md` for
connecting and testing rooms.
