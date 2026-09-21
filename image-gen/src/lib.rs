pub mod catalog;
pub mod cli;
pub mod config;
pub mod error;
pub mod eval;
pub mod generate;
pub mod hub;
pub mod image_io;
pub mod llm;
pub mod log_file;
pub mod logging;
pub mod models;
pub mod output;
pub mod preset;
pub mod report;
pub mod rewrite;
pub mod sdlog;

use std::path::{Path, PathBuf};

use cli::{Command, EvalMetric, GenerateArgs, ModelsCommand, RewriteMode};
use config::Config;
use error::AppError;
use eval::rank::{self, Borda};
use eval::{EvalConfig, ImageEval};
use llm::Llm;

/// One scored (or unscored, if `--eval` was not passed) generated image.
#[derive(Debug, Clone)]
pub struct GeneratedImage {
    /// `None` for a non-durable image (no `--output`, generated for `--eval` or
    /// `--show` only), mirroring the pre-existing `--json --show` empty-paths case.
    pub path: Option<PathBuf>,
    pub seed: i64,
    pub eval: ImageEval,
    /// `None` when neither `--eval` metric was requested (or both failed).
    pub borda: Option<Borda>,
}

#[derive(Debug, Clone, Default)]
pub struct EffectivePrompt {
    pub original: String,
    pub rewritten: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub images: Vec<GeneratedImage>,
    pub prompt: EffectivePrompt,
}

pub fn run(mut cli: cli::Cli) -> Result<Outcome, AppError> {
    let loaded = config::load(cli.config.as_deref())?;
    logging::init(&loaded.config)?;

    match &loaded.source {
        Some(path) => tracing::debug!(path = %path.display(), "loaded config"),
        None => tracing::debug!("no config file found; using defaults"),
    }
    tracing::debug!(
        models_dir = %loaded.config.models_dir.display(),
        log_dir = %loaded.config.log_dir.display(),
        "resolved config"
    );

    match &mut cli.command {
        Command::Generate(args) => {
            preset::apply(args, &loaded.config.presets)?;
            sdlog::init();
            generate_command(args, &loaded.config)
        }
        Command::Models { command } => {
            match command {
                ModelsCommand::Search(args) => catalog::search(args, &loaded.config.models_dir)?,
                ModelsCommand::List(args) => catalog::list(args, &loaded.config.models_dir)?,
            }
            Ok(Outcome::default())
        }
    }
}

/// Images that exist on disk right now. `_scratch` keeps the backing tempdir alive
/// for exactly as long as `paths` are valid; `None` when they are durable (written
/// to a user-chosen `--output`). `seed` is the base seed passed to `generate`;
/// `stable-diffusion.cpp` resolves copy `i`'s seed as `seed + i`.
struct Produced {
    paths: Vec<PathBuf>,
    seed: i64,
    durable: bool,
    _scratch: Option<tempfile::TempDir>,
}

/// `--seed` accepts a negative value to mean "randomize", matching
/// stable-diffusion.cpp's own convention, so that meaning is preserved here
/// rather than passing a negative seed through unchanged. Resolving in Rust
/// (rather than leaving it to the native side) makes the choice reportable and
/// fixes a real bug in the native fallback: `resolve_seed` there reseeds with
/// `srand(time(NULL))`, which is second-granularity and produces identical
/// images for two invocations within the same second.
fn resolve_seed(seed: Option<i64>) -> i64 {
    seed.filter(|s| *s >= 0).unwrap_or_else(random_seed)
}

fn random_seed() -> i64 {
    use std::hash::{BuildHasher, Hasher};
    (std::collections::hash_map::RandomState::new().build_hasher().finish() >> 33) as i64
}

const DEFAULT_EVAL_MAX_PX: u32 = 512;

fn generate_command(args: &GenerateArgs, config: &Config) -> Result<Outcome, AppError> {
    if args.json && args.show {
        tracing::warn!("--show writes image data to stdout, corrupting --json output");
    }
    require_checkpoint(args)?;

    let metrics: &[EvalMetric] = args.eval.as_deref().unwrap_or_default();
    let eval_requested = !metrics.is_empty();

    if args.output.is_none() && !args.show && !eval_requested {
        tracing::warn!("no --output path and no --show; image will not be output");
        return Ok(Outcome::default());
    }

    let rewrite_mode = args.rewrite.unwrap_or(RewriteMode::Auto);
    let rewrite_threshold = args.rewrite_threshold.unwrap_or(rewrite::DEFAULT_THRESHOLD);
    let will_rewrite = rewrite::should_rewrite(rewrite_mode, &args.prompt, rewrite_threshold);

    let llm = if eval_requested || will_rewrite {
        Some(Llm::new(&config.llm)?)
    } else {
        None
    };

    let rewritten = match (&llm, will_rewrite) {
        (Some(llm), true) => {
            let model = llm.model(args.rewrite_model.as_deref(), "rewrite-model")?;
            let text = rewrite::rewrite(llm, &model, &args.prompt)?;
            tracing::info!(original = %args.prompt, rewritten = %text, "rewrote prompt");
            Some(text)
        }
        _ => None,
    };
    let effective_prompt = rewritten.as_deref().unwrap_or(&args.prompt);

    let produced = produce(args, effective_prompt, &config.models_dir)?;

    // Only the metrics actually requested need a model resolved for them: a model
    // missing for an unrequested role (e.g. no `--judge-model` and no `[llm].model`
    // when only `--eval vqa` was passed) is not an error.
    let vqa_model = match &llm {
        Some(llm) if metrics.contains(&EvalMetric::Vqa) => {
            llm.model(args.vqa_model.as_deref(), "vqa-model")?
        }
        _ => String::new(),
    };
    let caption_model = match &llm {
        Some(llm) if metrics.contains(&EvalMetric::Tit) => {
            llm.model(args.caption_model.as_deref(), "caption-model")?
        }
        _ => String::new(),
    };
    let judge_model = match &llm {
        Some(llm) if metrics.contains(&EvalMetric::Tit) => {
            llm.model(args.judge_model.as_deref(), "judge-model")?
        }
        _ => String::new(),
    };
    let eval_config = EvalConfig {
        metrics,
        vqa_model: &vqa_model,
        caption_model: &caption_model,
        judge_model: &judge_model,
        max_px: args.eval_max_px.unwrap_or(DEFAULT_EVAL_MAX_PX),
    };

    let total_images = produced.paths.len();
    let scored: Vec<(Option<PathBuf>, i64, ImageEval)> = produced
        .paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let seed = produced.seed + index as i64;
            let eval = match &llm {
                Some(llm) if eval_requested => {
                    eval::run(llm, &eval_config, &args.prompt, path, index, total_images)
                }
                _ => ImageEval::default(),
            };
            let reported_path = produced.durable.then(|| path.clone());
            log_scored_image(reported_path.as_deref(), seed, &eval);
            (reported_path, seed, eval)
        })
        .collect();

    let evals: Vec<ImageEval> = scored.iter().map(|(_, _, eval)| eval.clone()).collect();
    let borda = rank::aggregate(&evals);

    let images: Vec<GeneratedImage> = scored
        .into_iter()
        .enumerate()
        .map(|(index, (path, seed, eval))| GeneratedImage {
            path,
            seed,
            eval,
            borda: borda.as_ref().map(|scores| scores[index].clone()),
        })
        .collect();

    if let Some(best) = best_scoring(&images) {
        tracing::info!(index = best, "best-scoring copy");
    }

    if args.show {
        let order = rank::display_order(&evals);
        let shown: Vec<&Path> = match &order {
            Some(order) => order.iter().map(|&i| produced.paths[i].as_path()).collect(),
            None => produced.paths.iter().map(PathBuf::as_path).collect(),
        };
        display_all(&shown, produced.durable)?;
    }

    Ok(Outcome {
        images,
        prompt: EffectivePrompt {
            original: args.prompt.clone(),
            rewritten,
        },
    })
}

fn log_scored_image(path: Option<&Path>, seed: i64, eval: &ImageEval) {
    tracing::info!(
        path = ?path.map(Path::display),
        seed,
        vqa = ?eval.vqa,
        tit = ?eval.tit,
        "scored image"
    );
}

/// The index of the image with Borda rank 1 (see `eval::rank`); `None` when no
/// image has any successful score to rank by.
fn best_scoring(images: &[GeneratedImage]) -> Option<usize> {
    images
        .iter()
        .position(|image| matches!(&image.borda, Some(borda) if borda.rank == 1))
}

/// `--model`/`--diffusion-model` used to be a required `ArgGroup`, but a preset
/// can supply either, so the check now runs after preset merging.
fn require_checkpoint(args: &GenerateArgs) -> Result<(), AppError> {
    if args.model.is_none() && args.diffusion_model.is_none() {
        return Err(AppError::NoCheckpoint);
    }
    Ok(())
}

/// Generate `args.copies` images of `prompt` (the effective prompt: rewritten, if
/// a rewrite happened, otherwise `args.prompt`), writing them to `args.output`
/// when set (a plain file for one copy, a filename prefix for more) or into a
/// scratch directory otherwise, so the caller can rely on `paths` being live
/// files either way.
fn produce(args: &GenerateArgs, prompt: &str, models_dir: &Path) -> Result<Produced, AppError> {
    match &args.output {
        Some(path) => produce_to_file(args, prompt, models_dir, path),
        None => produce_to_scratch(args, prompt, models_dir),
    }
}

fn produce_to_file(
    args: &GenerateArgs,
    prompt: &str,
    models_dir: &Path,
    path: &Path,
) -> Result<Produced, AppError> {
    if path.is_dir() {
        return Err(AppError::OutputIsDirectory {
            path: path.to_path_buf(),
        });
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }

    let seed = resolve_seed(args.seed);
    let dests = output::destinations(path, args.copies);

    if args.copies == 1 {
        generate::generate(args, prompt, seed, models_dir, &dests[0], 1)?;
        tracing::info!(path = %dests[0].display(), step = 1, total_steps = 1, "wrote image");
    } else {
        let scratch_parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let scratch = tempfile::Builder::new()
            .prefix(".image-gen-")
            .tempdir_in(scratch_parent)?;
        generate::generate(args, prompt, seed, models_dir, scratch.path(), args.copies)?;
        let sources = output::collect(scratch.path(), args.copies)?;
        let total_steps = dests.len();
        for (index, (src, dest)) in sources.iter().zip(&dests).enumerate() {
            output::place(src, dest)?;
            tracing::info!(path = %dest.display(), step = index + 1, total_steps, "wrote image");
        }
    }

    Ok(Produced {
        paths: dests,
        seed,
        durable: true,
        _scratch: None,
    })
}

/// Generate into a scratch directory that is deleted once the returned `Produced`
/// drops. Used for `--show` with no `-o`, and for score-only `--eval` runs.
fn produce_to_scratch(args: &GenerateArgs, prompt: &str, models_dir: &Path) -> Result<Produced, AppError> {
    let scratch = tempfile::tempdir()?;
    let seed = resolve_seed(args.seed);

    let paths = if args.copies == 1 {
        let path = scratch.path().join("image.png");
        generate::generate(args, prompt, seed, models_dir, &path, 1)?;
        vec![path]
    } else {
        generate::generate(args, prompt, seed, models_dir, scratch.path(), args.copies)?;
        output::collect(scratch.path(), args.copies)?
    };

    Ok(Produced {
        paths,
        seed,
        durable: false,
        _scratch: Some(scratch),
    })
}

fn display_all(paths: &[&Path], durable: bool) -> Result<(), AppError> {
    for (index, path) in paths.iter().enumerate() {
        if index > 0 {
            println!();
        }
        if durable {
            image_io::display_labeled(path)?;
        } else {
            image_io::display(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use cli::Cli;

    fn parse(args: &[&str]) -> GenerateArgs {
        let mut full = vec!["image-gen", "generate"];
        full.extend_from_slice(args);
        match Cli::try_parse_from(full).unwrap().command {
            Command::Generate(args) => *args,
            other => panic!("expected Command::Generate, got {other:?}"),
        }
    }

    #[test]
    fn model_satisfies_checkpoint() {
        let args = parse(&["a prompt", "--model", "stabilityai/sd-turbo"]);
        require_checkpoint(&args).unwrap();
    }

    #[test]
    fn diffusion_model_satisfies_checkpoint() {
        let args = parse(&["a prompt", "--diffusion-model", "stabilityai/sd-turbo"]);
        require_checkpoint(&args).unwrap();
    }

    #[test]
    fn neither_is_no_checkpoint() {
        let args = parse(&["a prompt"]);
        assert!(matches!(
            require_checkpoint(&args),
            Err(AppError::NoCheckpoint)
        ));
    }
}
