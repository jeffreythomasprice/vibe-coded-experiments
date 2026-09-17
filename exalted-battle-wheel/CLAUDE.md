# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

TODO.md is for humans. You can read it for context only if the user explicitly mentions it, and you can never update it.

README.md should be kept fairly terse. This shouldn't be explaining project architecture, just how to build and run it.

Prefer to avoid comments except when something is actually complicated. Avoid structural or conversational comments.

Prefer strict error handling with specific enums over something like anyhow.

Never run git commands that change repository or remote state — commit, add, branch, push, merge, rebase, reset, checkout to discard/switch, stash pop/drop, tag, etc. Investigating with git (`git diff`, `git log`, `git status`, `git show`, `git blame`) is fine and encouraged.

# Workspace layout

A Cargo workspace with three crates: `client` (the Leptos/wasm browser app, formerly the whole
project), `server` (an axum API), and `shared` (game logic and wire-protocol types used by both —
`shared::battle` is the rules engine, `shared::protocol` is what crosses the network). `client` and
`server` deploy independently — see "Hosting" below for the client and "Server hosting" for the
server.

# Commands

Full detail (including local access-code curl examples and the deploy sequence) is in README.md;
this is just the quick reference.

```
./dev.sh                        # runs dynamodb-local + server + client together; see README.md
cargo test --workspace          # all three crates, including client (native, not wasm)
cargo test -p shared battle::   # scope to one crate / module path; same pattern for server, client
cd client && trunk build --release   # production client bundle
```

`dev.sh` refuses to start if 8000 or 8001 are already bound, and needs `docker`, `aws`, `jq`,
`cargo`, and `trunk` on `PATH`.

# Architecture

## Wire protocol (`shared`)

Every DTO that crosses the network — REST bodies and websocket messages alike — is generated at
build time from JSON Schema (`shared/schemas/*.json`) by `shared/build.rs` via `typify`, landing in
`shared/src/generated.rs` and re-exported from `shared::access`/`rooms`/`protocol`. Never hand-edit
`generated.rs`; change the schema (see `shared/schemas/README.md` for the authoring rules) and
rebuild. A few types (`BattleLog`, `Timestamp`, `SessionRejection`, `Index`) are hand-written and
substituted in via `build.rs`'s `REPLACEMENTS` list instead of being generated, where typify can't
express what's needed.

Every decode — both directions, both crates — goes through `shared::validate::decode`, which checks
two independent layers before `serde` ever sees the bytes: typify's compiled-in bounds on named
string defs (`MemberName`, `RoomName`, ...), and a full JSON Schema validation against the exact
source document for whatever a compiled-in check can't express (cross-field rules). A malformed or
out-of-bounds message is rejected outright rather than partially deserialized.

Tagging is deliberately inconsistent between two families: the control-plane enums
(`ClientMessage`, `ServerMessage`, `BattleRequest`, `BattleCommand`, `ProtocolError`) are adjacently
tagged; `BattleEvent` (and `BattleLog`, which embeds a `Vec<BattleEvent>`) keep serde's external
tagging because `BattleLog` is persisted verbatim in the browser's `localStorage` (Solo mode) and in
the DynamoDB rooms table — retagging it would silently break every already-saved battle without a
migration.

`shared::battle` (the rules engine: `Combatant`, `BattleEvent`, `apply()`, `BattleLog`) has no
networking or IO of its own; `shared::protocol` is the layer that talks about connections, rooms,
and sessions on top of it. Both `client` and `server` depend on `shared` as a workspace path
dependency, so there is no separate DTO definition on each side to keep in sync by hand.

## Server (`server`, axum)

Three storage-agnostic traits — `AccessCodeStore`, `RoomStore`, `ConnectionStore` — each live in
their own module (`access_codes/`, `rooms/`, `connections/`) shaped the same way: a real `dynamo`
implementation and a `#[cfg(test)]`-only `memory` fake with matching conditional-write semantics.
`routes::AppState<A, R, C>` is generic over all three, so handlers are exercised end-to-end against
the memory fakes in `routes.rs`'s own test module without touching DynamoDB.

REST auth is a bearer access-code token in the `Authorization` header, checked by middleware
(`auth.rs`) before a handler runs. `/ws` can't do that — a browser `WebSocket` can't set custom
headers — so the upgrade itself is unauthenticated and every `ClientEnvelope` instead carries its
own token, checked per message inside `ws::handler::handle`; a socket that never sends one valid
message within 15s (`ws::AUTH_TIMEOUT`) is dropped.

`ws/` splits transport from logic:
- `ws/mod.rs` runs the actual per-connection read/write loop — mints a `ConnectionId`, spawns a
  writer task so a slow client can't block reads, tracks which room the connection is currently in.
- `ws/handler.rs` is pure dispatch: token check, permission check, `RoomStore` mutation, and the
  resulting list of `(ConnectionId, ServerMessage)` to send. It knows nothing about sockets, so it's
  unit-tested directly against the memory stores.
- `ws/hub.rs` is the in-process `ConnectionId → sender` registry used to fan a handler's output back
  out to every affected connection, plus the single global lock serializing every room mutation
  process-wide (see "Server hosting" below for why this pins the deployment to one replica).

## Client (`client`, Leptos/wasm)

Mirrors the server's transport/logic split: `net/` is the raw websocket transport (`Socket`) and
knows nothing about rooms or battles; `battle_net.rs`, built on top, tracks room membership, replays
a persisted session token to silently rejoin a room after a page reload, and reconciles local state
against whatever the server's last message actually said — the browser's own state is never
authoritative. `persist.rs`/`storage.rs` wrap `localStorage` for anything that must survive a
reload (a Solo-mode `BattleLog`, the room session token, prefs), each independently.

`Socket`'s callbacks fire from raw JS `WebSocket` events, outside any Leptos-tracked call stack —
every deferred handler in `battle_net.rs` re-enters the app's root `Owner` (captured once in
`app.rs`, see `ROOT_OWNER`'s doc comment) rather than spawning directly, or `use_context` would
silently find nothing.

Build-time config (`API_BASE_URL`) is baked into the wasm bundle as an `env!` constant by
`client/build.rs`, which layers `client/.env` < `.env.<profile>` < `.env.local` (`<profile>` is
`development` under `trunk serve`, `production` under `trunk build --release`) — see `config.rs`.

`ui/` is one module per panel/widget (`roster.rs`, `wheel.rs`, `queue.rs`, `action_panel.rs`, ...),
composed together in `app.rs`. `Tip`/`DetailTip` plus `glossary.rs` are the teaching-tooltip
mechanism referenced in "Game rules / references" below.

# Hosting

The app is hosted as a static site at `exalted.jeffrey.lol`: Route 53 alias → CloudFront (OAC) → private S3 bucket `jeff-exalted-battle-wheel`. Nothing in the bucket is public; CloudFront is the only reader. Infrastructure is defined in `terraform/`, scoped entirely to this project (its own bucket, distribution, and certificate) — no resources are shared with other projects yet.

`terraform apply`/`destroy` and `./deploy.sh` are human-only actions. `deploy.sh` deploys the client, the server, or both (default; `./deploy.sh client` or `./deploy.sh server` to do just one) — see "Server hosting" below for what its server half does. Claude may write and edit the Terraform config and the deploy script, but must never run `terraform init`/`plan`/`apply`/`destroy` or `deploy.sh` itself — report what a change would do and let the user run it. Working AWS credentials for this project are in the `personal` CLI profile, not the default one.

Terraform state lives in `s3://jeffs-tfstate/exalted-battle-wheel/terraform.tfstate`, using S3's native locking (`use_lockfile`, no DynamoDB table). That state bucket was created by hand and is deliberately not managed by this Terraform config, since the config can't create the bucket its own state lives in.

The TLS certificate is a DNS-validated wildcard for `*.jeffrey.lol` in us-east-1 (required region for CloudFront certs); it covers `exalted.jeffrey.lol` but not the apex `jeffrey.lol`.

Caching contract: Trunk content-hashes every asset filename except `index.html`, so hashed assets are uploaded with a long immutable `Cache-Control` and `index.html` is uploaded with `no-cache` — every deploy invalidates `/*` so `index.html` picks up the new hashes immediately. The `.wasm` file's `Content-Type` is forced to `application/wasm` explicitly rather than trusted to the uploading tool's MIME guess, since a wrong type silently breaks `instantiateStreaming`.

Trunk emits `index.html` with absolute root-relative asset paths and Subresource Integrity hashes on the script/style/wasm tags. That means the app must be served from the domain root (no path prefix), and CloudFront must never rewrite response bodies — its gzip/brotli compression is fine since SRI checks the decoded body, not the wire bytes.

# Server hosting

The server is a stateless axum app on the shared k0s cluster in `../kubernetes-host` (see that
repo's `README.md`/`CLAUDE.md` for how the cluster itself is provisioned and reached), at
`exalted-api.jeffrey.lol`. Deliberately one label deep: `*.jeffrey.lol` is a wildcard Route53 A
record to the cluster's Elastic IP, and a wildcard matches only a single label, so
`api.exalted.jeffrey.lol` would not resolve without a new record. `exalted.jeffrey.lol` itself is
an exact-match alias to this repo's own CloudFront distribution, which beats the wildcard — so the
client (above) and the server can never collide on a hostname.

The node is arm64, so the image is always built with `docker buildx build --platform linux/arm64`
(cargo-chef caches the dependency layer in `server/Dockerfile` so a code-only change doesn't
recompile the whole tree). There is no container registry: `../kubernetes-host/push-image.sh`
streams a locally-built image straight into the node's containerd, and `deploy.sh`'s server half
requires that repo's kubectl tunnel already running and `KUBECONFIG` already exported, since it
can't set either of those up itself.

Every deploy tags the image `v<UTC timestamp>` and never reuses a tag: `deploy.sh` builds and
pushes that image, then substitutes it into `server/manifest.yaml`'s `image:` line before
`kubectl apply`ing the result (the checked-in `exalted-server:v1` is just a placeholder for a
manual `kubectl apply -f server/manifest.yaml`). This matters because there's no registry — the
image tag is the only thing that tells the node "this is different from what's already running,"
so a fixed tag would need a manual `kubectl rollout restart` to pick up new bits, and `:latest`
would make kubelet default to pulling from Docker Hub instead of using what was just imported.

TLS is cert-manager/Let's Encrypt via the same HTTP-01 ClusterIssuers every app on that cluster
uses, not the ACM certificate above. `server/manifest.yaml` starts on `letsencrypt-staging`; after
`kubectl apply`, `kubectl get certificate -w` until it shows `READY True`, then
`curl -k https://exalted-api.jeffrey.lol/health` (`-k` because the staging cert isn't in any local
trust store). Once that's confirmed, flip the Ingress's `cert-manager.io/cluster-issuer` annotation
to `letsencrypt-prod`, `kubectl apply` again, wait for the new certificate, and drop `-k`.

The cluster has no StorageClass and no working PVCs, and pods can't reach the EC2 instance role
(IMDS hop limit 1) for any AWS access either — so the server must stay stateless; anything durable
needs a deliberate decision (an external database, S3, etc.), not local disk.

`server/manifest.yaml`'s Deployment is pinned to `replicas: 1` and must stay that way: room
membership broadcast goes through an in-process sender registry keyed by connection id, and every
room mutation is serialized by a single in-process lock (`server/src/ws/hub.rs`). A second pod
would neither receive broadcasts meant for connections on the first, nor actually serialize writes
against it. Scaling this out for real needs a pub/sub layer (e.g. DynamoDB Streams, or Redis) and a
distributed lock, not a replica count bump.

The server signs the room session token every websocket member is handed on `Joined`
(`server/src/sessions.rs`) with `SESSION_SECRET`, delivered the same way as the DynamoDB
credentials above: a Terraform-managed `random_password` (`terraform/sessions.tf`), upserted by
`deploy.sh` into the `exalted-server-session` Secret on every deploy. Unlike the AWS credentials,
this secret must be created by `terraform apply` at least once *before* the first `./deploy.sh
server` that reads it — a fresh `terraform apply` for this project always needs to precede a deploy
for that reason.

Actually deploying — `deploy.sh`'s server half, `../kubernetes-host/push-image.sh`, `kubeconfig.sh`,
`tunnel.sh`, and any `kubectl` command against that cluster's real kubeconfig — is human-only, same
as `terraform apply`/`destroy` above. Claude may write and edit `server/Dockerfile`,
`server/manifest.yaml`, and `deploy.sh`, and may build and run the image locally
(`docker buildx build ... --load`, `docker run`) to verify it, but must never run `deploy.sh`, the
scripts above, or `kubectl` against the cluster — report what a change would do and let the user
run it.

# Game rules / references

RULES.md is research notes on Exalted 2E combat: the tick system, action Speeds and DV penalties, attack resolution, and the data a battle needs. Start there for combat questions — it cites printed page numbers, so it also tells you where to look in the books.

It is a summary, not a substitute for the sources. Go back to the rule books whenever RULES.md is unclear, doesn't cover the question, or flags something as uncertain (its weapon tables in particular are OCR-damaged and marked as such). Supplementing or correcting RULES.md from the sources is fine and encouraged.

To look things up, use the `document-search` skill. Always pass `--tag exalted` so the search is scoped to the Exalted 2E books (as opposed to unrelated documents in the corpus). Note that the PDF page index is the printed page number + 2.

When a new feature is backed by a rule from the sources, add a teaching tooltip to its UI widget explaining how it works and citing the page number it comes from. Before shipping the tooltip, double-check the cited content and page number against the actual rule book (not just RULES.md) for accuracy.
