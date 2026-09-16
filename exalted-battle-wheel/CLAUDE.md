TODO.md is for humans. You can read it for context only if the user explicitly mentions it, and you can never update it.

REAMDE.md should be kept fairly terse. This shouldn't be explaining project architecture, just how to build and run it.

Prefer to avoid comments except when something is actually complicated. Avoid structural or conversational comments.

Prefer strict error handling with specific enums over something like anyhow.

# Workspace layout

A Cargo workspace with three crates: `client` (the Leptos/wasm browser app, formerly the whole
project), `server` (an axum API), and `shared` (game logic and wire-protocol types used by both —
`shared::battle` is the rules engine, `shared::protocol` is what crosses the network). `client` and
`server` deploy independently — see "Hosting" below for the client and "Server hosting" for the
server.

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
