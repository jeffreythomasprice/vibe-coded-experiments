# Automated evaluation of generated images

Research notes, 2026-09-20. Covers the two evaluation methods under consideration:

- **Method A** — image → text, then compare the text against the original prompt.
- **Method B** — image → image, compared directly against a reference image.

Both are feasible. They answer different questions and neither is a general
"is this image good" score.

## Framing

"Evaluate a generated image" is four separate questions, and a metric that is
good at one is usually near-random at the others:

| Axis | Question | Covered by |
|---|---|---|
| Alignment | Did it draw what I asked for? | **Method A** |
| Reference similarity | Does it match this image? | **Method B** |
| Preference / quality | Is it any good? | Neither — see [What neither method measures](#what-neither-method-measures) |
| Technical defects | Mangled hands, garbled text | Neither |

Two measurements that make the point: HPSv2 (a human-preference model) scores
Kendall τ = 9.0 on GenAI-Bench alignment — near-random. CLIPScore scores
Spearman ρ = 0.09 against human quality rankings. There is no single number.

---

# Method A — image to text, compared against the prompt

## The idea is real and has a name

Captioning a generated image and comparing that caption to the prompt is
**text-to-image-to-text (TIT) consistency**, also called cycle-consistency
evaluation. It comes in two quite different forms, and the intuitive one is not
the one to reach for first.

## A1. VQAScore — the recommended default

Rather than generating a caption, ask the model a yes/no question and read the
probability of the answer token:

```
Does this figure show "{prompt}"? Please answer yes or no.
```

Generate **one** token with logprobs enabled, then:

```
score = P("Yes") / (P("Yes") + P("No"))
```

That is the entire metric. It comes from *Evaluating Text-to-Visual Generation
with Image-to-Text Generation* (arXiv 2404.01291, ECCV 2024) — note that the
paper's own title frames it as an image-to-text method, so this is the same
instinct as the caption idea, with the caption replaced by a single token.

Reported pairwise accuracy against human judgment on TIFA160:

| Metric | Pairwise accuracy |
|---|---|
| CLIPScore | 54.1% |
| GPT-4V as a judge | 64.0% |
| ImageReward | 67.3% |
| **VQAScore** | **71.2%** |

On GenAI-Bench: 62.8% pairwise, Kendall τ = 37.2.

**The 11B CLIP-FlanT5 from the paper is not required.** Its own ablation puts
InstructBLIP at 62.3 and LLaVA-1.5 at 61.6 against CLIP-FlanT5's 62.8 — the
method carries the gain, not the backbone. A 3–4B VLM at Q4_K_M is ~2.5 GB plus
a ~0.5–0.9 GB mmproj.

**Why it beats the caption form on typical prompts.** A caption is an
information bottleneck. For *"a red cube on a blue sphere"*, a captioner may
write "a cube and a sphere on a table" — and a missing attribute in the
*caption* is indistinguishable from a missing attribute in the *image*.
Conditioning on the claim makes the model go looking for that specific thing.

**Known weakness:** VQAScore saturates near 1.0, so it discriminates poorly
among candidates that are all already decent — which is exactly the best-of-N
regime. Useful as a gate ("is this acceptable?"), weaker as a fine-grained
ranker.

## A2. TIT-Score — for long prompts

The literal version of the original idea, and state of the art in its niche.
From *TIT-Score* (arXiv 2510.02987, Oct 2025): a VLM writes a 250–350 word
description of the image **without being shown the prompt and without being
asked to score anything**, then that description is compared against the
original prompt.

On LPG-Bench (200 prompts averaging 250+ words, 2,600 images, 12,832 human
preference pairs):

| Metric | Pairwise accuracy |
|---|---|
| CLIPScore | 48.5% — **below chance** |
| BLIP2-ITM | 53.6% |
| VQAScore | 58.2% |
| prior SOTA | 59.2% |
| **TIT-Score** | **66.4%** |

Why it wins here: decoupling perception from judgment. The captioner is never
told what answer you want, so it cannot yes-bias, and the scorer compares two
texts rather than compressing a 250-word prompt into one similarity number.

Two engineering findings from that paper worth copying: a Qwen2.5-VL-**7B**
captioner beat the 32B one (scale is not the lever), and a 4B embedding model
was only marginally worse than the 8B.

Corroborating evidence that the cycle signal is real and not a one-benchmark
artifact: **CycleReward** (arXiv 2506.02095, ICCV 2025) used it to generate
866K preference pairs with **zero human labels** and trained a reward model
reaching 65% human agreement.

## If implementing A2, compare with an LLM, not a cosine

A sentence-embedding cosine is largely order-insensitive, so *"the horse is
eating the grass"* and *"the grass is eating the horse"* land in nearly the
same place. Using an embedding cosine moves CLIP's bag-of-words blindness from
the image-text step to the text-text step instead of removing it. Have an LLM
check each clause of the prompt against the caption.

n-gram metrics (BLEU / CIDEr / ROUGE) are worse still.

## Other failure modes of the caption form

- **Omission is invisible.** See the red-cube example above.
- **Captioner hallucination inflates scores.** VL-RewardBench found most
  VLM-judge errors are *perceptual*, not reasoning — a small captioner simply
  does not see fine detail.
- **Uncalibrated, style-sensitive scale.** `"cinematic photo, 8k, bokeh, a
  wizard"` and a faithful prose description are lexically distant even at
  perfect alignment. TIT-Score partly dodges this by fixing caption length and
  benchmarking on long prompts; a naive short-prompt version is dominated by
  style-token mismatch.
- **Two models' errors compound**, and a bad score cannot be attributed to the
  generator vs. the captioner.
- **~300× more generated tokens** than VQAScore's single token.

## Do not use CLIPScore as the alignment metric

Its absolute scale is meaningless (raw cosines cluster ~0.15–0.40 with no zero
point, and shift with prompt length). An invariance audit (arXiv 2605.24702)
found vertical flips move it 6–8%, moving an object corner-to-corner moves it
7–9%, and ±10° rotation 4–6% — on perturbations humans judged semantically
equivalent 97.3% of the time. When two systems are within 0.7%, spatial
perturbations flip the ranking 28–36% of the time.

It is acceptable for ranking seeds under one fixed prompt. Never report the
number, never compare across prompts.

## Implementation path

The blocker is that this needs a VLM, not an encoder. Three options:

1. **`llama-server` as a managed subprocess** *(recommended)*. Speaks OpenAI
   `/v1/chat/completions` with image content parts, accepts a local file path,
   and returns top-token logprobs via `n_probs` — which is exactly what
   VQAScore needs. No FFI, no ggml symbol collision, crashes are isolated and
   restartable, and `hub.rs` already does the model + mmproj download.
2. **`llama-cpp-2` with the `mtmd` feature in-process.** Cleanest UX, typed
   error enums that match this repo's style. Risk: this binary already
   statically links stable-diffusion.cpp's ggml, and llama.cpp vendors a
   different ggml revision — a real duplicate-symbol hazard.
3. **A cloud VLM API.** Roughly $1–2 per 1000 images on a small model,
   downscaling to 512² first and using a batch endpoint. For a hobby project
   this is often cheaper than the engineering cost of the local path.

**Model choice note:** Gemma 3 uses a fixed 256 image tokens regardless of
input size, which makes latency predictable — a genuinely good property for a
scorer. Qwen2.5-VL / Qwen3-VL are dynamic (~1337 tokens at 1024²), so clamp
`image_max_tokens` to 256–512. That is the single biggest latency lever.

Expected: ~0.5–1.5 s/image for a 3B-class VLM on CUDA. Watch for the log line
`the CLIP graph uses unsupported operators by the backend` — that warning is
the difference between 1 s and 60 s.

---

# Method B — image compared against a reference image

## Mechanism

Run both images through a frozen vision encoder, L2-normalize the embeddings,
take the cosine. One ONNX model, two forward passes, one dot product. Tens of
milliseconds on CPU — negligible next to sampling.

This is the easier of the two methods to build and should be built first.

## Encoder choice

**Recommended: DINOv3 ViT-S/16.**

`onnx-community/dinov3-vits16-pretrain-lvd1689m-ONNX` — verified ungated
(the upstream Meta repo is gated; this mirror is not):

| File | Size |
|---|---|
| `onnx/model.onnx_data` (fp32) | 86.3 MB |
| `onnx/model_quantized.onnx_data` (int8) | 21.8 MB |
| `onnx/model_q4.onnx_data` (q4) | 14.7 MB |

**The weights live in external-data files.** `onnx/model.onnx` is only ~0.1 MB
— it is just the graph. ONNX Runtime resolves `model.onnx_data` by *relative
path at load time*, so the fetch must pull both files and keep them adjacent in
the cache directory. Fetching only the `.onnx` produces a model that loads and
then fails.

Preprocessing: RGB → bicubic resize short side to 224/256 → center crop → CHW,
scale to [0,1] → normalize with ImageNet mean `[0.485, 0.456, 0.406]` and std
`[0.229, 0.224, 0.225]`.

### Why DINO-family and not CLIP

The reason is structural, not empirical tuning. CLIP is trained against
captions, so it is invariant to exactly the details captions omit — which is
most of what "is this the same dog" depends on. DINO is self-supervised to
distinguish images from each other, so it preserves instance-level detail. Two
different golden retrievers score high on CLIP-I and noticeably lower on DINO.

Human 2AFC agreement on "are these the same thing" (NIGHTS dataset):

| Metric | Agreement |
|---|---|
| LPIPS | 70.7% |
| CLIP | 83.1% |
| OpenCLIP | 87.6% |
| **DINO** | **90.1%** |
| DreamSim (tuned ensemble) | 96.2% |

Alternatives, if the DINOv3 custom license is a problem: **DINOv2 is Apache-2.0**
and only slightly behind. If a text-aligned space is wanted (so the same encoder
can also do prompt-adherence scoring), use **SigLIP 2**, not CLIP.

## Do not use PSNR, SSIM, or LPIPS

PSNR and SSIM assume **pixel-wise registration**. A generated image is a
different image of the same idea — the subject is a few pixels left, the arm is
at a different angle. Each of those is a large penalty and zero perceptual
difference. A blurred image can beat a sharper but shifted one. They answer
"how much was this image degraded?", which is not the question.

**LPIPS has the same disease, milder.** It was trained on BAPPS — *small
distortions of the same image* (JPEG, noise, blur, superresolution artifacts).
The ECCV 2022 shift-tolerance paper showed a **single-pixel translation,
imperceptible to a human, measurably corrupts LPIPS**. Between two
independently generated images it is essentially measuring low-level texture
similarity. Its 70.7% above is barely above chance on this task.

If a genuinely low-level metric is wanted anyway, use **DISTS**, which
deliberately trades pixel-alignment sensitivity for texture-resampling
tolerance in order to evaluate generative output.

## The scalar conflation problem

**A single similarity number conflates subject, style, composition, palette and
lighting.** Two images can both score 0.85 because they share a subject — or
because they share a beige background.

This is the documented failure of the DINO score in the DreamBooth literature:
it rises when generations match the reference on background, palette, lighting
and texture, *not* the subject. A model that simply memorizes the reference
scores near-perfectly while completely failing to place the subject in a novel
context. DreamBench++ (arXiv 2406.16855, ICLR 2025) showed systematically that
DINO and CLIP-I "often misalign with human preferences."

**Mitigation: report 2–3 numbers, not one.**

| Score | How | Measures |
|---|---|---|
| `content` | DINOv3 cosine | subject / instance identity |
| `style` | CSD cosine, or a Gram-matrix score as a cheap stand-in | brushwork, texture, palette |
| `color` | LAB-mean / histogram distance | nearly free, and explains away many spurious high content scores |

Style is a genuinely separate question from subject, and a subject encoder will
not answer it. **CSD** (Contrastive Style Descriptors, arXiv 2404.01292) is the
purpose-built tool — ViT-L, 768-d descriptors, trained with augmentations
curated to preserve style.

Caveat on CSD: a 2026 diagnostic (arXiv 2605.09030) found raw CSD cosine
unreliable as an *absolute* style-fidelity score — negative discrimination gaps
for 23/91 artists — and the failure is shared across CLIP-ViT-L, SigLIP-large
and DINOv2-Large backbones, so it is not CSD-specific. It is fine for **ranking
within a batch**, which is this use case.

For character consistency specifically, generic embeddings are the wrong tool —
use ArcFace / InsightFace face embeddings. Note the licensing: InsightFace's
*code* is MIT but the `buffalo_l` *weights* derive from research-only datasets
and require a separate commercial license.

## Scores are only comparable within one batch

Absolute values mean nothing and drift between models and prompts. Compare
candidates generated from the same prompt with the same model, and nothing
else. This should be stated wherever the number is surfaced.

## Optional upgrade: DreamSim

**DreamSim** (arXiv 2306.09344, MIT licensed) is purpose-built for exactly this
"are these the same thing" judgment and reaches 96.2% human agreement. The
single-branch `dino_vitb16` variant is ~3× faster than the ensemble and is
straightforward to export to ONNX once the LoRA is merged.

There is **no published ONNX export**, which is the only friction. Worth doing
if the DINOv3 ranking ever visibly disagrees with your eye.

---

# Shared infrastructure

## Runtime: `ort`

`ort` 2.0.0-rc.13 (wraps ONNX Runtime 1.30) is the path for every
encoder-shaped scorer in Method B, and for any CLIP-family scorer.

- `download-binaries` is **in the default feature set** — `cargo add ort` pulls
  a statically-linked runtime at build time. No CMake, no system package, no
  `LD_LIBRARY_PATH`.
- Execution providers are Cargo features. **`features = ["webgpu"]` has a
  prebuilt Linux x86_64 binary and runs on the Vulkan stack this project
  already requires** — no CUDA toolkit needed. `features = ["cuda"]` also has a
  prebuilt (CUDA 13 + TensorRT) if the toolkit is ever installed.
- It has been at `2.0.0-rc.N` since mid-2024. That reflects the maintainer
  declining to freeze the API while ORT churns, not abandonment — ~7M downloads
  per quarter, and most of the Rust embedding ecosystem depends on it. Pin an
  rc and accept churn on upgrade.

`open_clip_inference` 0.4.2 is a useful reference implementation (or something
to vendor) for the preprocessing and session plumbing.

For a pure-Rust alternative, `candle` 0.11 ships `clip`, `siglip`, `dinov2`,
`blip`, `llava`, `moondream` and `paligemma` examples, and its `clip` / `siglip`
models expose `get_image_features` / `get_text_features` directly. It has no
Vulkan backend (an unanswered proposal was opened 2026-09-16) and its CPU VLM
performance is unusable — candle's own moondream README reports 0.68 tok/s.

Python should appear exactly once, in a throwaway `uv run` script with PEP 723
inline metadata that exports a checkpoint to ONNX. Never at runtime.

## What is *not* available from stable-diffusion.cpp

Verified against the vendored source: the CLIP vision tower exists
(`src/model/te/clip.hpp` defines `CLIPVisionModel` and
`CLIPVisionModelProjection`) and `sd_ctx_params_t` accepts a `clip_vision_path`
— but it is wired internally for PhotoMaker / PuLID / IP-Adapter conditioning.
**None of the 53 `SD_API` functions returns an embedding or feature vector.**

Exposing one would mean adding an export to the C++, regenerating bindings, and
threading a patch through `fetch-source.sh`, which re-downloads pristine
source. Not worth it against a 3-line ONNX download.

---

# What neither method measures

## Preference / quality

Neither method says whether an image is *good*. For that, the default is
**HPSv2.1** — the only option with a clean Apache-2.0 grant across code,
weights (`xswu/HPSv2`) *and* the HPDv2 dataset. It is a stock CLIP
architecture, and `adams-story/HPSv2-hf` has already converted it to a
`transformers` CLIPModel, so the ONNX export is a solved path. ~1B params,
~2 GB fp16.

Honest caveat: it scores 65.3% on HPDv3, barely above the ImageReward-era
baseline on modern generators. HPSv3 reaches 76.9% but is a 7B Qwen2-VL with no
ONNX path and a license that is self-contradictory across four sources.

**Do not use LAION Aesthetic V2 as a selector.** It scores **56.8% on Pick-a-Pic
pairwise, where the random baseline is 56.8%** — exactly at chance. It is the
most widely used scorer in the SD ecosystem and it does not work for ranking.
Compute it for ecosystem-comparable logging; give it zero weight in decisions.
An audit (arXiv 2601.09896) also found it returns *zero* images ≥6.5 from
African, Oceanian, Native American, Egyptian, Islamic or Ancient West Asian
collections.

Note `discus0434/aesthetic-predictor-v2-5` is **AGPL-3.0**, and the head weights
live in that repo.

## Technical defects

**No preference model detects mangled hands.** *Understanding Reward Hacking in
Text-to-Image RL* (arXiv 2601.03468) tested every available reward model and
found 39–68% accuracy at picking the artifact-free image of a pair — some below
chance — and states that ensembling "can only partially mitigate this issue."

The best available option is **DiffDoctor** (ICCV 2025) — SegFormer-B5,
per-pixel artifact heatmap, MIT licensed, 339 MB fp32. No ONNX is shipped but
SegFormer exports cleanly. Note the upstream `nvidia/mit-b5` encoder carries a
non-commercial license.

Use it as a **veto, not a score**: filter first, rank the survivors. Maximizing
an artifact score invites hacking it; a hard gate does not.

Two specific traps: do not threshold on hand-keypoint *confidence* (MediaPipe
reports 0.936 on visibly malformed hands vs 0.948 on repaired ones — a ~1pp
delta across a night-and-day difference), and do not use an LM-decoder OCR to
detect garbled text (TrOCR / Florence-2 / Qwen-VL decode autoregressively and
launder gibberish into the nearest plausible real word, erasing the exact defect
being hunted). Use CTC-based OCR instead.

---

# Using these scores in a loop

Brief notes; this deserves its own document if it gets built.

**Best-of-N selection is the safe regime.** It has a provable KL bound: N=16 →
≤1.84 nats and ≤94.1% win rate; N=64 → ≤3.18 nats and ≤98.5%. That is 1.34
extra nats for 4.4 points. **N=8–16 captures most of the gain.**

**`--copies N` already produces the candidates.** `stable-diffusion.cpp`
computes `cur_seed = request.seed + b` per batch item, so N copies are N
distinct deterministic seeds, and the winning candidate *i* is exactly
reproducible later as `--seed <base+i> --copies 1`.

**The API constrains which algorithms are reachable.** `sd_img_gen_params_t`
exposes `seed`, `init_image`, `strength`, `batch_count` and `ref_images` — but
**no initial-latent input**. That rules out zero-order noise search, DNO,
Golden Noise / NPNet, FK steering and search-over-paths, all of which write
latents or intervene mid-trajectory. The reachable move set is: random search
over `seed`, prompt variants, and `init_image` + `strength` refinement.

That is fine — Ma et al. (arXiv 2501.09732) found zero-order and path search
gave only marginal gains over plain random search at text-to-image scale, and
Flash-BoN (arXiv 2607.04461) found that prior work flattered guided search by
comparing at matched NFE; under matched **wall-clock**, plain best-of-N matches
or beats it.

**Rules worth following:**

- **Stop on patience, not a threshold.** These scores are not calibrated
  per-prompt, so a fixed threshold loops forever on hard prompts and exits
  instantly on easy ones. Stop when best-so-far has not improved in k=4–8
  candidates, or N_max is reached.
- **Cap N.** True quality is **unimodal in N** under an imperfect proxy — it
  rises, peaks, then declines (arXiv 2506.19248). Past the peak, more search
  actively selects worse images.
- **Keep a held-out verifier you never optimize against.** Select with A, report
  B. If B falls while A rises, you are hacking A. One extra forward pass.
- **Borda-aggregate ranks, never average scores.** Reward scales are
  uncalibrated and incomparable; ranks are not.
- **Never select on aesthetic alone.** Measured in Ma et al.: searching on
  Aesthetic raised Aesthetic 5.79 → 6.38 but *lowered* CLIPScore 0.71 → 0.69.
  Searching on CLIPScore raised it to 0.82 but *lowered* Aesthetic to 5.68. The
  two actively damage each other.
- **Spread the budget across axes.** 4 prompt variants × 8 seeds beats 32 seeds
  on one prompt; Flash-BoN measured prompt optimization stacking with BoN
  (+8% → +16%).
- **Log every candidate and its scores.** Scoring is ~1% of generation cost, so
  keeping everything allows re-ranking with a better verifier later without
  regenerating.

**The one line not to cross:** feeding selected outputs back into *model
updates* compounds without bound. The ImageReward paper measured exactly that
(RAFT): win rate 49.86% at iteration 1 → **20.97% at iteration 3**, worse than
the base model. This applies to training a LoRA on winners. It does *not* apply
to resampling fresh seeds, nor to bounded `init_image` refinement with decaying
strength (the CoHP scheme in HPSv3 reports an 87% user-study win rate over plain
selection).

## Do not use FID per image

At N=1 the generated-side covariance is *identically* the zero matrix, so FID
provably collapses to `‖φ(x) − μ_ref‖² + constant` — squared distance to the
reference centroid. That is **minimized by the most generic, prototypical image
in the space**: a smooth grey blur near the centroid beats a sharp, correct
image in a legitimate off-centre mode. It ranks backwards.

KID at n=1 is literally 0/0. CMMD's shipped implementation silently returns a
Parzen-window typicality score instead of erroring, which is worse. FID needs
>20,000 samples to be stable at all, and no unbiased estimator exists at any N.

---

# References

**Method A**
- VQAScore — https://arxiv.org/abs/2404.01291 · https://github.com/linzhiqiu/t2v_metrics
- TIT-Score — https://arxiv.org/abs/2510.02987
- CycleReward — https://arxiv.org/abs/2506.02095
- CLIPScore — https://arxiv.org/abs/2104.08718
- CLIPScore invariance audit — https://arxiv.org/abs/2605.24702
- TIFA — https://arxiv.org/abs/2303.11897 · DSG — https://arxiv.org/abs/2310.18235
- GenEval 2 / benchmark drift — https://arxiv.org/abs/2512.16853
- llama.cpp multimodal — https://github.com/ggml-org/llama.cpp/blob/master/docs/multimodal.md

**Method B**
- DreamSim — https://arxiv.org/abs/2306.09344 · https://github.com/ssundaram21/dreamsim
- DINOv3 — https://github.com/facebookresearch/dinov3 · ONNX: https://huggingface.co/onnx-community/dinov3-vits16-pretrain-lvd1689m-ONNX
- SigLIP 2 — https://arxiv.org/abs/2502.14786
- CSD — https://arxiv.org/abs/2404.01292 · diagnostic: https://arxiv.org/abs/2605.09030
- LPIPS shift intolerance — https://www.ecva.net/papers/eccv_2022/papers_ECCV/papers/136780089.pdf
- DISTS — https://github.com/dingkeyan93/DISTS
- DreamBooth — https://arxiv.org/abs/2208.12242 · DreamBench++ — https://arxiv.org/abs/2406.16855

**Quality, artifacts, loops**
- HPSv2 — https://github.com/tgxs002/HPSv2 · HPSv3 — https://arxiv.org/abs/2508.03789
- PickScore — https://github.com/yuvalkirstain/PickScore
- LAION aesthetic — https://github.com/christophschuhmann/improved-aesthetic-predictor
- DiffDoctor — https://arxiv.org/abs/2501.12382 · https://github.com/ali-vilab/DiffDoctor
- Reward hacking in T2I RL — https://arxiv.org/abs/2601.03468
- Inference-time scaling — https://arxiv.org/abs/2501.09732
- Flash-BoN — https://arxiv.org/abs/2607.04461
- BoN KL bound — https://arxiv.org/abs/2401.01879
- Inference-time reward hacking — https://arxiv.org/abs/2506.19248
- OPT2I (prompt optimization) — https://arxiv.org/abs/2403.17804

**Runtime**
- `ort` — https://github.com/pykeio/ort · https://ort.pyke.io
- `candle` — https://github.com/huggingface/candle
- `open_clip_inference` — https://github.com/RuurdBijlsma/open-clip-inference-rs
