in-flight:

I want to implement both VQAScore and TIT-Score. See @docs/evaluation.md for more details.

Specifically I want the ability to:
- provide a large prompt for image generation
- have an llm model turn the large prompt into a shorter more precise propmt suitable for image generation; e.g. this might rely excise unnecessary detail or summarize a complicated prompt into something suitable
- generate multiple images at once, basically the `--copies` argument
- for each generated copy, run the specific evaluation algorithm (VQA or TIT) and include that output in our logging output, or in our `--json` output

If it's feasible to do both evaluation metrics at once then we can treat cli args as an inclusive or, e.g. `--eval vga --eval tit` in one prompt.

We will need to include any other cli args necessary to define the evaluation method, e.g. the llm models to use to summarize the prompts and another model that can be used to do the image-to-text step.

We should allow the eval model specification to be done via `--preset`. Since we probably want to separate the eval cli args from the image generation cli args, we should allow multiple `--preset` arguments and have them combine silently. Warn if they both define the same arg, keep last preset. Our config.toml.example should include a preset that defines all the eval args.

Our README.md should include an example of doing evals.


todo:

pipeline that uses evals to repeatedly generate images until they match some criteria


make a skill
use --json


from docs/evaluation.md
Method B — image compared against a reference image