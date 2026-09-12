TODO.md is for humans. You can read it for context only if the user explicitly mentions it, and you can never update it.

REAMDE.md should be kept fairly terse. This shouldn't be explaining project architecture, just how to build and run it.

Prefer to avoid comments except when something is actually complicated. Avoid structural or conversational comments.

Prefer strict error handling with specific enums over something like anyhow.

# Hosting

The app is hosted as a static site at `exalted.jeffrey.lol`: Route 53 alias → CloudFront (OAC) → private S3 bucket `jeff-exalted-battle-wheel`. Nothing in the bucket is public; CloudFront is the only reader. Infrastructure is defined in `terraform/`, scoped entirely to this project (its own bucket, distribution, and certificate) — no resources are shared with other projects yet.

`terraform apply`/`destroy` and `./deploy.sh` are human-only actions. Claude may write and edit the Terraform config and the deploy script, but must never run `terraform init`/`plan`/`apply`/`destroy` or `deploy.sh` itself — report what a change would do and let the user run it. Working AWS credentials for this project are in the `personal` CLI profile, not the default one.

Terraform state lives in `s3://jeffs-tfstate/exalted-battle-wheel/terraform.tfstate`, using S3's native locking (`use_lockfile`, no DynamoDB table). That state bucket was created by hand and is deliberately not managed by this Terraform config, since the config can't create the bucket its own state lives in.

The TLS certificate is a DNS-validated wildcard for `*.jeffrey.lol` in us-east-1 (required region for CloudFront certs); it covers `exalted.jeffrey.lol` but not the apex `jeffrey.lol`.

Caching contract: Trunk content-hashes every asset filename except `index.html`, so hashed assets are uploaded with a long immutable `Cache-Control` and `index.html` is uploaded with `no-cache` — every deploy invalidates `/*` so `index.html` picks up the new hashes immediately. The `.wasm` file's `Content-Type` is forced to `application/wasm` explicitly rather than trusted to the uploading tool's MIME guess, since a wrong type silently breaks `instantiateStreaming`.

Trunk emits `index.html` with absolute root-relative asset paths and Subresource Integrity hashes on the script/style/wasm tags. That means the app must be served from the domain root (no path prefix), and CloudFront must never rewrite response bodies — its gzip/brotli compression is fine since SRI checks the decoded body, not the wire bytes.

# Game rules / references

RULES.md is research notes on Exalted 2E combat: the tick system, action Speeds and DV penalties, attack resolution, and the data a battle needs. Start there for combat questions — it cites printed page numbers, so it also tells you where to look in the books.

It is a summary, not a substitute for the sources. Go back to the rule books whenever RULES.md is unclear, doesn't cover the question, or flags something as uncertain (its weapon tables in particular are OCR-damaged and marked as such). Supplementing or correcting RULES.md from the sources is fine and encouraged.

To look things up, use the `document-search` skill. Always pass `--tag exalted` so the search is scoped to the Exalted 2E books (as opposed to unrelated documents in the corpus). Note that the PDF page index is the printed page number + 2.

When a new feature is backed by a rule from the sources, add a teaching tooltip to its UI widget explaining how it works and citing the page number it comes from. Before shipping the tooltip, double-check the cited content and page number against the actual rule book (not just RULES.md) for accuracy.
