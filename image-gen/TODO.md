in-flight:


todo:

I want to make a skill in .claude/skills that runs this project. It should expect the already built binary to be available on the path, so don't try to hard-code paths to the target dir.

If the user is asking to generate an image we should try to invoke the tool and given them the path to the best image. This means we should use `--copies` and `--eval` to generate 

We should expect an `image-eval-skill` preset to exist that defines the appropriate models and other parameters. If the command fails because no such preset exists propmt the user to create one. If no such preset exists we should propmt the user how to assemble one. Make sure the skill has an example of such a preset that includes our current preferred settings:
```
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
model = "qwen3:4b"
```

We should invoke with `--copies 5` unless the user expresses a preference for a specific number.

We should invoke with `--json` and parse the output to find the "best" image. We're looking to order by something like `.images[].borda.rank`. Example output is like:
```
{
  "prompt": {
    "original": "a red bicycle on a beach"
  },
  "images": [
    {
      "path": "/tmp/bike0.png",
      "seed": 1449317825,
      "vqa": {
        "score": 0.9541667
      },
      "tit": {
        "score": 1.0,
        "claims": [
          {
            "claim": "a red bicycle",
            "verdict": "supported"
          },
          {
            "claim": "on a beach",
            "verdict": "supported"
          }
        ]
      },
      "borda": {
        "vqa": 1.0,
        "tit": 1.5,
        "total": 2.5,
        "rank": 3
      }
    },
    {
      "path": "/tmp/bike1.png",
      "seed": 1449317826,
      "vqa": {
        "score": 0.9683192
      },
      "tit": {
        "score": 1.0,
        "claims": [
          {
            "claim": "a red bicycle",
            "verdict": "supported"
          },
          {
            "claim": "on a beach",
            "verdict": "supported"
          }
        ]
      },
      "borda": {
        "vqa": 3.0,
        "tit": 1.5,
        "total": 4.5,
        "rank": 1
      }
    },
    {
      "path": "/tmp/bike2.png",
      "seed": 1449317827,
      "vqa": {
        "score": 0.9448878
      },
      "tit": {
        "score": 1.0,
        "claims": [
          {
            "claim": "a red bicycle",
            "verdict": "supported"
          },
          {
            "claim": "on a beach",
            "verdict": "supported"
          }
        ]
      },
      "borda": {
        "vqa": 0.0,
        "tit": 1.5,
        "total": 1.5,
        "rank": 4
      }
    },
    {
      "path": "/tmp/bike3.png",
      "seed": 1449317828,
      "vqa": {
        "score": 0.95942104
      },
      "tit": {
        "score": 1.0,
        "claims": [
          {
            "claim": "a red bicycle",
            "verdict": "supported"
          },
          {
            "claim": "on a beach",
            "verdict": "supported"
          }
        ]
      },
      "borda": {
        "vqa": 2.0,
        "tit": 1.5,
        "total": 3.5,
        "rank": 2
      }
    }
  ],
  "totalTime": 290.128
}
```


pipeline that uses evals to repeatedly generate images until they match some criteria


from docs/evaluation.md
Method B — image compared against a reference image