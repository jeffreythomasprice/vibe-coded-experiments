---
name: generate-image
description: Generate an image from a text prompt using the image-gen CLI. Produces several candidates, scores them with VQA and TIT-Score, and returns the best-ranked one. Use whenever the user asks to generate, render, draw, or make an image or picture from a description.
argument-hint: "<image prompt>"
---

# Generate an image with image-gen

Turns an image request into one `image-gen generate` invocation that produces
several candidates, scores them, and hands back the best one. Assumes the
`image-gen` binary is already built and on `PATH` — never hard-code a path
into `target/debug` or `target/release`, and never fall back to
`cargo run --`.

## 1. Invoke it

Pick a short filename slug from the prompt (a few words, hyphenated). Write
output to a fresh temp directory per run unless the user asked for a specific
location:

```bash
outdir=$(mktemp -d /tmp/image-gen-XXXXXX)
image-gen generate "<the prompt>" \
  --preset image-eval-skill \
  --copies 5 \
  -o "$outdir/<slug>.png" \
  --json > "$outdir/result.json" 2> "$outdir/run.log"
```

Rules:

- **`--copies 5`** unless the user asked for a specific number — use theirs
  instead.
- **`-o` is mandatory.** Without it, images land in a scratch directory that's
  deleted when the process exits, and the JSON omits `path` entirely.
- **Never combine `--show` with `--json`** — `--show` writes image bytes to
  stdout and corrupts the JSON output. Don't pass `--show`; view the winning
  file afterward instead (step 4).
- With `--copies N > 1`, `-o` becomes a filename **prefix**: `slug.png`
  becomes `slug0.png` … `slug{N-1}.png` (zero-padded once N > 10).
- Pass through any per-invocation flags the user explicitly asks for
  (`-W`/`-H`, `--steps`, `--seed`, `--negative`, etc.). Model choice and eval
  settings belong in the preset (see below), not on the command line.
- **Run this in the background** (`run_in_background: true` on the Bash
  call). A `--copies 5` run with `--eval tit` is 5 diffusion passes plus 2
  LLM calls per image; a cold model cache also means a multi-gigabyte
  HuggingFace download and/or an Ollama model pull first. This routinely
  exceeds a foreground command timeout. Poll or wait for completion, then
  read `$outdir/result.json`.

## 2. Parse the result and pick the winner

`--json` prints a report to stdout. On success:

```json
{
  "prompt": {"original": "...", "rewritten": "... (only if the prompt was compressed)"},
  "images": [
    {"path": "...", "seed": 123, "vqa": {"score": 0.95}, "tit": {"score": 1.0, "claims": [...]},
     "borda": {"vqa": 3.0, "tit": 1.5, "total": 4.5, "rank": 1}},
    ...
  ],
  "totalTime": 290.128
}
```

**`borda.rank` is 1-based, and rank 1 is the BEST image** (descending by
`borda.total`; ties share a rank). `borda.vqa`/`borda.tit` are Borda points,
not the raw metric scores — the raw scores are the sibling `vqa.score` /
`tit.score` on the same image. `borda` is entirely absent from an image if
eval wasn't requested or every scoring call for it failed; treat those as
unranked, not rank-1.

```bash
# best image's path
jq -r '.images | sort_by(.borda.rank // 1e9) | .[0].path' "$outdir/result.json"

# full ranked table
jq -r '.images | sort_by(.borda.rank // 1e9)[]
       | "rank \(.borda.rank // "-")  vqa \(.vqa.score // "-")  tit \(.tit.score // "-")  \(.path // "(not saved)")"' \
  "$outdir/result.json"
```

If the top-level output has an `"error"` key instead of `"prompt"`/`"images"`,
generation failed — see Troubleshooting below instead of parsing further.

## 3. Report back

- State the winning image's path plainly, its rank, and its vqa/tit scores.
- Read the winning PNG so it renders inline in the conversation.
- List the other copies' paths (don't delete them) in case the user wants to
  compare.
- If `prompt.rewritten` is present, mention the prompt was compressed before
  generating (it still gets scored against the original).

## 4. The `image-eval-skill` preset

This invocation expects a preset named `image-eval-skill` to exist in the
user's `config.toml` (first found of: `--config` path, `./config.toml`, the
binary's own directory, then `~/.config/image-gen/config.toml`). **If the
command fails with `unknown preset 'image-eval-skill'`, don't substitute a
different preset or add flags to work around it** — tell the user the preset
is missing and offer to add this block to their config file (ask before
editing a config file you didn't create this session):

```toml
[presets.image-eval-skill]
diffusion_model = "city96/FLUX.1-dev-gguf:flux1-dev-F16.gguf"
clip_l = "comfyanonymous/flux_text_encoders:clip_l.safetensors"
t5xxl = "comfyanonymous/flux_text_encoders:t5xxl_fp8_e4m3fn.safetensors"
vae = "unsloth/FLUX.1-dev:ae.safetensors"
weight_type = "q8_0"
eval = ["vqa", "tit"]
rewrite = "auto"
rewrite_threshold = 300
eval_max_px = 512
vqa_model = "qwen3-vl:4b"
caption_model = "qwen3-vl:4b"
rewrite_model = "qwen3:4b"
judge_model = "qwen3:4b"

# Not a preset key — --eval/--rewrite need Ollama reachable here, and TIT-Score's
# caption call often runs past the 120s default.
[llm]
backend = "ollama"

[llm.ollama]
timeout_secs = 300
```

If the user has no `config.toml` at all yet, point them at
`config.toml.example` in the project root as a fuller reference (it also
covers `models_dir`, logging, and other presets) and offer to create a
minimal one containing just this preset plus `[llm]`/`[llm.ollama]`.

## Troubleshooting

- `unknown preset 'image-eval-skill'; config defines: ...` — see step 4.
- `no checkpoint: pass --model or --diffusion-model, or a --preset that sets one`
  — the preset exists but doesn't set diffusion weights; check it wasn't
  edited down to just the eval keys.
- `no model for --<flag>: pass it explicitly, set [llm].model in config.toml, or use a preset that sets one`
  — an eval/rewrite role has no model resolved; check the preset's
  `vqa_model`/`caption_model`/`rewrite_model`/`judge_model` or set `[llm].model`.
  Only `vqa_model`/`caption_model` need a vision-capable model.
- A connection error to `localhost:11434` (or wherever `[llm.ollama]` points)
  means Ollama isn't running; `--eval` and prompt rewriting both need it.
- `--output <path> is a directory; pass a file path` — `-o` must name a file,
  not a directory.
- Per-image scores are also written to the rotating log under `log_dir`
  (default `/tmp/image-gen/logs`) if you need more detail than the JSON gives.
