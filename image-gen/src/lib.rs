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
pub mod sd;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cli::{Command, EvalMetric, GenerateArgs, ModelsCommand, RewriteMode, ServerCommand};
use config::Config;
use error::AppError;
use eval::rank::{self, Borda};
use eval::{EvalConfig, ImageEval};
use generate::{CopyParams, UsedParams};
use llm::Llm;

/// One scored (or unscored, if `--eval` was not passed) generated image.
#[derive(Debug, Clone)]
pub struct GeneratedImage {
    /// `None` for a non-durable image (no `--output`, generated for `--eval` or
    /// `--show` only), mirroring the pre-existing `--json --show` empty-paths case.
    pub path: Option<PathBuf>,
    pub seed: i64,
    pub params: UsedParams,
    pub eval: ImageEval,
    /// `None` when neither `--eval` metric was requested (or both failed).
    pub borda: Option<Borda>,
}

struct ScoredImage {
    path: Option<PathBuf>,
    seed: i64,
    params: UsedParams,
    eval: ImageEval,
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

pub async fn run(mut cli: cli::Cli) -> Result<Outcome, AppError> {
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
            generate_command(args, &loaded.config).await
        }
        Command::Models { command } => {
            match command {
                ModelsCommand::Search(args) => catalog::search(args, &loaded.config.models_dir)?,
                ModelsCommand::List(args) => catalog::list(args, &loaded.config.models_dir)?,
            }
            Ok(Outcome::default())
        }
        Command::Server { command } => {
            server_command(command, &loaded.config.sd_server, &loaded.config.log_dir).await?;
            Ok(Outcome::default())
        }
    }
}

/// Images that exist on disk right now. `_scratch` keeps the backing tempdir alive
/// for exactly as long as `paths` are valid; `None` when they are durable (written
/// to a user-chosen `--output`). `copies[i]` is the seed and params actually used
/// for `paths[i]`, in order.
struct Produced {
    paths: Vec<PathBuf>,
    copies: Vec<CopyParams>,
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

async fn generate_command(args: &GenerateArgs, config: &Config) -> Result<Outcome, AppError> {
    let command_started = Instant::now();
    if args.json && args.show {
        tracing::warn!("--show writes image data to stdout, corrupting --json output");
    }
    require_checkpoint(args)?;
    require_ref_images_exist(args)?;
    require_valid_jitter(args)?;

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
        Some(Llm::new(&config.llm))
    } else {
        None
    };

    let rewritten = match (&llm, will_rewrite) {
        (Some(llm), true) => {
            let model = llm.model(args.rewrite_model.as_deref(), "rewrite-model")?;
            let text = rewrite::rewrite(llm, &model, &args.prompt).await?;
            tracing::info!(original = %args.prompt, rewritten = %text, "rewrote prompt");
            Some(text)
        }
        _ => None,
    };
    let effective_prompt = rewritten.as_deref().unwrap_or(&args.prompt);

    let gen_started = Instant::now();
    let produced = produce(args, effective_prompt, config).await?;
    let gen_elapsed = gen_started.elapsed();

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
    let mut scored: Vec<ScoredImage> = Vec::with_capacity(total_images);
    let mut vqa_total = Duration::ZERO;
    let mut vqa_count = 0u32;
    let mut tit_total = Duration::ZERO;
    let mut tit_count = 0u32;
    for (index, path) in produced.paths.iter().enumerate() {
        let copy = produced.copies[index];
        let eval = match &llm {
            Some(llm) if eval_requested => {
                let (eval, timing) =
                    eval::run(llm, &eval_config, &args.prompt, path, index, total_images).await;
                if let Some(elapsed) = timing.vqa {
                    vqa_total += elapsed;
                    vqa_count += 1;
                }
                if let Some(elapsed) = timing.tit {
                    tit_total += elapsed;
                    tit_count += 1;
                }
                eval
            }
            _ => ImageEval::default(),
        };
        let reported_path = produced.durable.then(|| path.clone());
        log_scored_image(reported_path.as_deref(), copy.seed, &eval);
        scored.push(ScoredImage {
            path: reported_path,
            seed: copy.seed,
            params: copy.params,
            eval,
        });
    }

    let evals: Vec<ImageEval> = scored.iter().map(|image| image.eval.clone()).collect();
    let borda = rank::aggregate(&evals);

    let images: Vec<GeneratedImage> = scored
        .into_iter()
        .enumerate()
        .map(|(index, scored)| GeneratedImage {
            path: scored.path,
            seed: scored.seed,
            params: scored.params,
            eval: scored.eval,
            borda: borda.as_ref().map(|scores| scores[index].clone()),
        })
        .collect();

    if let Some(best) = best_scoring(&images) {
        tracing::info!(index = best, "best-scoring copy");
    }

    log_timing_summary(
        command_started.elapsed(),
        gen_elapsed,
        total_images,
        vqa_total,
        vqa_count,
        tit_total,
        tit_count,
    );

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

/// Logs the run's timing breakdown once image generation and scoring are both
/// done: an overall total, image generation's total and per-image average,
/// and — only when at least one image was scored — evaluation's total and
/// per-image average, further split out per metric.
fn log_timing_summary(
    total: Duration,
    image_gen: Duration,
    image_count: usize,
    vqa_total: Duration,
    vqa_count: u32,
    tit_total: Duration,
    tit_count: u32,
) {
    tracing::info!(
        total_secs = total.as_secs_f64(),
        image_gen_secs = image_gen.as_secs_f64(),
        image_gen_avg_secs = image_gen.as_secs_f64() / image_count as f64,
        "generation timing"
    );

    if vqa_count == 0 && tit_count == 0 {
        return;
    }
    let eval_total = vqa_total + tit_total;
    tracing::info!(
        eval_secs = eval_total.as_secs_f64(),
        eval_avg_secs = eval_total.as_secs_f64() / image_count as f64,
        "eval timing"
    );
    if vqa_count > 0 {
        tracing::info!(
            vqa_secs = vqa_total.as_secs_f64(),
            vqa_avg_secs = vqa_total.as_secs_f64() / vqa_count as f64,
            "vqa eval timing"
        );
    }
    if tit_count > 0 {
        tracing::info!(
            tit_secs = tit_total.as_secs_f64(),
            tit_avg_secs = tit_total.as_secs_f64() / tit_count as f64,
            "tit eval timing"
        );
    }
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

/// Checked up front so a typo'd `--ref-image` path fails fast with a clear
/// local error, rather than surfacing later as an opaque read error after a
/// daemon may already have been spawned for this generation.
fn require_ref_images_exist(args: &GenerateArgs) -> Result<(), AppError> {
    for path in &args.ref_image {
        if !path.is_file() {
            return Err(AppError::RefImageNotFound { path: path.clone() });
        }
    }
    Ok(())
}

/// A preset can supply `jitter` too, bypassing clap's range checking, so this
/// runs post-merge like `require_checkpoint`.
fn require_valid_jitter(args: &GenerateArgs) -> Result<(), AppError> {
    if let Some(value) = args.jitter
        && !(0.0..=1.0).contains(&value)
    {
        return Err(AppError::JitterOutOfRange { value });
    }
    Ok(())
}

/// Generate `args.copies` images of `prompt` (the effective prompt: rewritten, if
/// a rewrite happened, otherwise `args.prompt`), writing them to `args.output`
/// when set (a plain file for one copy, a filename prefix for more) or into a
/// scratch directory otherwise, so the caller can rely on `paths` being live
/// files either way.
async fn produce(args: &GenerateArgs, prompt: &str, config: &Config) -> Result<Produced, AppError> {
    match &args.output {
        Some(path) => produce_to_file(args, prompt, config, path).await,
        None => produce_to_scratch(args, prompt, config).await,
    }
}

async fn produce_to_file(
    args: &GenerateArgs,
    prompt: &str,
    config: &Config,
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
    let generated = generate::generate(args, prompt, seed, config, args.copies).await?;

    let total_steps = dests.len();
    for (index, (dest, image)) in dests.iter().zip(&generated).enumerate() {
        output::write_image(dest, &image.bytes)?;
        tracing::info!(path = %dest.display(), step = index + 1, total_steps, "wrote image");
    }

    Ok(Produced {
        paths: dests,
        copies: generated.into_iter().map(|image| image.copy).collect(),
        durable: true,
        _scratch: None,
    })
}

/// Generate into a scratch directory that is deleted once the returned `Produced`
/// drops. Used for `--show` with no `-o`, and for score-only `--eval` runs.
async fn produce_to_scratch(args: &GenerateArgs, prompt: &str, config: &Config) -> Result<Produced, AppError> {
    let scratch = tempfile::tempdir()?;
    let seed = resolve_seed(args.seed);
    let generated = generate::generate(args, prompt, seed, config, args.copies).await?;

    let mut paths = Vec::with_capacity(generated.len());
    for (index, image) in generated.iter().enumerate() {
        let path = scratch.path().join(format!("image{index}.png"));
        output::write_image(&path, &image.bytes)?;
        paths.push(path);
    }

    Ok(Produced {
        paths,
        copies: generated.into_iter().map(|image| image.copy).collect(),
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

async fn server_command(command: &ServerCommand, config: &sd::SdConfig, log_dir: &Path) -> Result<(), AppError> {
    match command {
        ServerCommand::Status => match sd::daemon::status(config).await {
            Some(status) if status.alive => {
                println!("running (pid {})", status.state.pid);
                println!("release tag: {}", status.state.tag);
                println!("fingerprint: {}", status.state.fingerprint);
                println!("log file: {}", status.state.log_path.display());
            }
            Some(status) => {
                println!(
                    "recorded daemon (pid {}) is not responding; run `image-gen server stop` to clear it",
                    status.state.pid
                );
            }
            None => println!("not running"),
        },
        ServerCommand::Stop => {
            if sd::daemon::stop(config).await? {
                println!("stopped");
            } else {
                println!("not running");
            }
        }
        ServerCommand::Restart => {
            if sd::daemon::stop(config).await? {
                println!("stopped; the next `generate` will start a fresh daemon");
            } else {
                println!("not running; the next `generate` will start one");
            }
        }
        ServerCommand::Logs { follow } => print_logs(config, log_dir, *follow).await?,
    }
    Ok(())
}

async fn print_logs(config: &sd::SdConfig, log_dir: &Path, follow: bool) -> Result<(), AppError> {
    let path = match sd::daemon::status(config).await {
        Some(status) => status.state.log_path,
        None => log_dir.join("sd-server.log"),
    };

    let contents = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    print!("{contents}");
    if !follow {
        return Ok(());
    }

    let mut offset = contents.len() as u64;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let Ok(metadata) = tokio::fs::metadata(&path).await else {
            continue;
        };
        if metadata.len() <= offset {
            continue;
        }
        let Ok(mut file) = tokio::fs::File::open(&path).await else {
            continue;
        };
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        if file.seek(std::io::SeekFrom::Start(offset)).await.is_err() {
            continue;
        }
        let mut buf = Vec::new();
        if file.read_to_end(&mut buf).await.is_ok() {
            print!("{}", String::from_utf8_lossy(&buf));
            offset = metadata.len();
        }
    }
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

    #[test]
    fn no_ref_images_is_fine() {
        let args = parse(&["a prompt"]);
        require_ref_images_exist(&args).unwrap();
    }

    #[test]
    fn existing_ref_image_is_fine() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let args = parse(&["a prompt", "--ref-image", file.path().to_str().unwrap()]);
        require_ref_images_exist(&args).unwrap();
    }

    #[test]
    fn missing_ref_image_errors() {
        let args = parse(&["a prompt", "--ref-image", "/no/such/file.png"]);
        match require_ref_images_exist(&args) {
            Err(AppError::RefImageNotFound { path }) => {
                assert_eq!(path, PathBuf::from("/no/such/file.png"));
            }
            other => panic!("expected RefImageNotFound, got {other:?}"),
        }
    }

    #[test]
    fn no_jitter_is_valid() {
        let args = parse(&["a prompt"]);
        require_valid_jitter(&args).unwrap();
    }

    #[test]
    fn jitter_zero_is_valid() {
        let args = parse(&["a prompt", "--jitter", "0"]);
        require_valid_jitter(&args).unwrap();
    }

    #[test]
    fn negative_jitter_errors() {
        let args = parse(&["a prompt", "--jitter=-0.1"]);
        match require_valid_jitter(&args) {
            Err(AppError::JitterOutOfRange { value }) => assert_eq!(value, -0.1),
            other => panic!("expected JitterOutOfRange, got {other:?}"),
        }
    }

    #[test]
    fn jitter_above_one_errors() {
        let args = parse(&["a prompt", "--jitter", "1.5"]);
        match require_valid_jitter(&args) {
            Err(AppError::JitterOutOfRange { value }) => assert_eq!(value, 1.5),
            other => panic!("expected JitterOutOfRange, got {other:?}"),
        }
    }
}
