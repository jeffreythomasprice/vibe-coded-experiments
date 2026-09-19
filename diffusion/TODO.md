in-flight:

We have some ability to automatically pull models from a web resource. I want to interact with whatever that web resource's API is, or scrape their data, or otherwise be able to search their models. This should be an independent subcommand for us, that produces lists of possible models that we could pull by name. We should show in that output whether we already have the model downloaded, and how big it is. There should be a flag on this subcommand to only show models we already have downloaded, i.e. what models that if generated require no further network.

We should move the existing cli args to a new subcommand called "generate". There should be no default subcommand. This means that existing invocations will fail as they won't provide either subcommand. Update README.md and CLAUDE.md accordingly.


todo:

pipeline that does image comparison or analysis against repeated attempts
like an automated system that takes a prompt and then generates several and auto-evals each attempt against some metric
then we have two different prompts and want to pick the one of each that is closest to each other, or the one that closest to the theme of an existing set
like we're doing pixel art and we have a new sprite sheet and want to pick the ones that are in the same style as the rest
