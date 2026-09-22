# image-gen

Generate images by driving a local `sd-server` process.

## Build

```
cargo build
cargo build --release
```

Generation talks over HTTP to `sd-server`, from
[leejet/stable-diffusion.cpp](https://github.com/leejet/stable-diffusion.cpp). The first
`generate` run downloads a prebuilt release automatically and keeps it running in the
background as a daemon so later runs skip reloading the model; see `image-gen server
--help` to check on it, stop it, or follow its log. `--backend cpu|vulkan|cuda` selects
the compute device an already-running daemon uses; there is no `cuda` release for
Linux, so that value only works on Windows builds.

## Run

```
cargo run -- generate "a chess piece beating another chess piece with a baseball bat" \
	--diffusion-model city96/FLUX.1-dev-gguf:flux1-dev-F16.gguf \
	--clip-l comfyanonymous/flux_text_encoders:clip_l.safetensors \
	--t5xxl comfyanonymous/flux_text_encoders:t5xxl_fp8_e4m3fn.safetensors \
	--vae unsloth/FLUX.1-dev:ae.safetensors \
	--weight-type q8_0 \
	-o /tmp/chess.png \
	--show

cargo run -- generate "a red bicycle on a beach" \
	--model stabilityai/sd-turbo \
	--steps 4 \
	--cfg-scale 1 \
	--guidance 0 \
	-o /tmp/bike.png \
	--show
```

`--model` (or `--diffusion-model` for standalone diffusion weights) accepts a
HuggingFace ref (`owner/repo`, `owner/repo:file`, `owner/repo@revision:file`) or a
local path; weights auto-download into `models_dir` on first use. `--copies N`
generates N images, turning `-o` into a filename prefix (`bike.png` → `bike0.png`,
`bike1.png`, …). See `cargo run -- generate --help` for the full flag list.

Flag bundles that only depend on the model can live in `config.toml` as named
presets, reducing the above to `generate "a red bicycle on a beach" --preset
turbo -o /tmp/bike.png --show`. `--preset` is repeatable and combines left to
right; explicit flags, and later presets, override earlier ones. See
`config.toml.example`.

## Generating from reference images

```
cargo run -- generate "put the logo on the mug" \
	--diffusion-model <a FLUX.1 Kontext, FLUX.2, or Qwen-Image-Edit checkpoint> \
	--ref-image /tmp/logo.png \
	--ref-image /tmp/mug.png \
	-o /tmp/mug-with-logo.png \
	--show
```

`--ref-image` is repeatable and passes each image to the model as in-context
conditioning alongside the prompt. It only does something with a model built for
it (FLUX.1 Kontext, FLUX.2, Qwen-Image-Edit); other models ignore it. These are
tagged `image-to-image` on HuggingFace, so find one with `models search --all`
rather than the default `models search`, which only looks at `text-to-image`.

## Evaluating generated images

```
cargo run -- generate "a red bicycle on a beach" \
	--preset turbo \
	--eval vqa --eval tit \
	--copies 4 -o /tmp/bike.png \
	--json \
	--show
```

`--eval vqa` and `--eval tit` score each copy against the prompt (repeatable,
both may be passed); `--rewrite auto` (the default) compresses a long prompt
before generating and scores against the original. All three need a
vision-capable model reachable through `[llm]` in `config.toml` (see
`config.toml.example`'s `[llm]` section). Scores land in the log per copy and,
with `--json`, in each image's `vqa`/`tit` fields. TIT-Score is slow — a
250–350 word caption call per image, then a judge call — so start with a small
`--copies` and `--eval vqa` alone before reaching for `--eval tit`. If you find
yourself passing the same `--eval`/`--rewrite*` flags often, bundle them into a
preset (see `config.toml.example`'s `eval` preset) instead of retyping them.

## Finding models

```
cargo run -- models search turbo      # search HuggingFace, marking what's already downloaded
cargo run -- models list              # list only what's already downloaded, no network
```

`models search` caches HuggingFace's responses under `models_dir/.hub-cache`; pass
`--refresh` to bypass that cache and re-query.

## Testing

`cargo test` runs the unit suite. `cargo test --features ollama-tests` also runs
integration tests against a real Ollama server expected at `localhost:11434`.
`cargo test --features sd-server-tests` runs the `sd-server` daemon lifecycle suite
against a lightweight stand-in binary — no download or real model needed.
