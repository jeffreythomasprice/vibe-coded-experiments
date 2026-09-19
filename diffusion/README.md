# diffusion

Generate images with diffusion-rs.

## Build

The default build uses the Vulkan backend, which needs Vulkan and SPIR-V headers
installed (`glslc` and `libvulkan.so` are not enough on their own):

```
sudo pacman -S vulkan-headers spirv-headers   # or your distro's equivalent
vendor/diffusion-rs-sys/fetch-source.sh   # once, after cloning
cargo build
```

For a CPU-only build, or CUDA:

```
cargo build --no-default-features
cargo build --no-default-features --features cuda
```

The first build compiles stable-diffusion.cpp and is slow. Switching feature sets
(e.g. vulkan to cpu) forces a full rebuild of it.

## Run

```
cargo run -- "a red bicycle on a beach" --model stabilityai/sd-turbo \
  --steps 4 --cfg-scale 1 --guidance 0 -o bike.png --show
```

`--model` (or `--diffusion-model` for standalone diffusion weights) accepts a
HuggingFace ref (`owner/repo`, `owner/repo:file`, `owner/repo@revision:file`) or a
local path; weights auto-download into `models_dir` on first use. See
`cargo run -- --help` for the full flag list.
