mod jitter;

use std::path::PathBuf;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::cli::{BatchFallback, GenerateArgs, ImageProvider, Sampler};
use crate::config::Config;
use crate::error::AppError;
use crate::gemini::batch::{self, BatchOutcome};
use crate::gemini::{self, GeminiError};
use crate::gemini::image::ImageRequest;
use crate::llm::Image;
use crate::models::{self, ModelRef};
use crate::sd::client::{Guidance, ImgGenRequest, SampleParams};
use crate::sd::modelset::ModelSet;
use crate::sd::{SdError, daemon, progress};
pub use jitter::UsedParams;

/// The seed and sampling params actually sent for one copy.
#[derive(Debug, Clone, Copy)]
pub struct CopyParams {
    pub seed: i64,
    pub params: UsedParams,
}

/// One generated image alongside the params that produced it.
#[derive(Debug)]
pub struct Generated {
    /// Position among the requested copies; can skip values when a batch
    /// delivered only some of them.
    pub index: u32,
    pub bytes: Vec<u8>,
    pub copy: CopyParams,
}

pub fn provider(args: &GenerateArgs, config: &Config) -> ImageProvider {
    args.provider.unwrap_or(config.image.provider)
}

pub async fn generate(
    args: &GenerateArgs,
    prompt: &str,
    seed: i64,
    config: &Config,
    copies: u32,
) -> Result<Vec<Generated>, AppError> {
    match provider(args, config) {
        ImageProvider::SdServer => generate_sd(args, prompt, seed, config, copies).await,
        ImageProvider::Gemini => generate_gemini(args, prompt, seed, config, copies).await,
    }
}

/// Seeded `seed..seed+copies`. Gemini has no sampling params to report or
/// jitter, so each copy's `UsedParams` is empty.
///
/// Interactive copies are sequential and fail fast. With `--batch` the copies
/// go out as one batch job instead; there a copy that fails is skipped with a
/// warning, and the run only fails if no copy produced an image.
async fn generate_gemini(
    args: &GenerateArgs,
    prompt: &str,
    seed: i64,
    config: &Config,
    copies: u32,
) -> Result<Vec<Generated>, AppError> {
    let client = gemini::Client::new(&config.gemini)?;
    let ref_images = args
        .ref_image
        .iter()
        .map(|path| Image::from_path(path))
        .collect::<Result<Vec<_>, _>>()?;
    let request_for = |index: u32| ImageRequest {
        prompt,
        ref_images: &ref_images,
        seed: seed + i64::from(index),
        width: args.width,
        height: args.height,
    };
    let generate_one = |index: u32| {
        let client = &client;
        let request = request_for(index);
        async move {
            let bytes = gemini::image::generate(client, &config.image.gemini, &request).await?;
            tracing::info!(
                index = index + 1,
                copies,
                seed = request.seed,
                model = %config.image.gemini.model,
                "generated image"
            );
            Ok::<_, AppError>(generated(index, seed, bytes))
        }
    };

    if args.batch != Some(true) {
        let mut images = Vec::with_capacity(copies as usize);
        for index in 0..copies {
            images.push(generate_one(index).await?);
        }
        return Ok(images);
    }

    let mut images = Vec::new();
    let mut errors = Vec::new();
    let mut retry = Vec::new();

    let wait = args.batch_wait.map(|wait| wait.0).unwrap_or(batch::DEFAULT_WAIT);
    let fallback = args.batch_fallback.unwrap_or(BatchFallback::Interactive);
    let ratio = gemini::image::aspect_ratio(args.width, args.height)?;
    let items = (0..copies)
        .map(|index| {
            let body = gemini::image::request_body(&request_for(index), &config.image.gemini, ratio);
            (index, body)
        })
        .collect();
    let outcome = batch::run_batch(
        &client,
        &config.image.gemini.model,
        items,
        wait,
        config.gemini.batch_poll(),
    )
    .await?;

    let results = match outcome {
        BatchOutcome::Done(results) => results,
        BatchOutcome::TimedOut { name, partial } => match fallback {
            BatchFallback::Fail => return Err(GeminiError::BatchTimedOut { name, waited: wait }.into()),
            BatchFallback::Interactive => {
                tracing::warn!(batch = %name, ?wait, "batch did not finish in time; generating the rest interactively");
                retry = (0..copies)
                    .filter(|index| !partial.iter().any(|result| result.index == *index))
                    .collect();
                partial
            }
        },
        BatchOutcome::Unavailable { bytes } => match fallback {
            BatchFallback::Fail => return Err(GeminiError::BatchTooLarge { bytes }.into()),
            BatchFallback::Interactive => {
                tracing::warn!(bytes, "batch request too large; generating interactively");
                retry = (0..copies).collect();
                Vec::new()
            }
        },
    };

    for result in results {
        match result.result {
            Ok(bytes) => {
                tracing::info!(
                    index = result.index + 1,
                    copies,
                    seed = seed + i64::from(result.index),
                    model = %config.image.gemini.model,
                    "generated image (batch)"
                );
                images.push(generated(result.index, seed, bytes));
            }
            Err(error) => {
                tracing::warn!(
                    index = result.index + 1,
                    seed = seed + i64::from(result.index),
                    %error,
                    "copy failed in batch"
                );
                errors.push(format!("copy {}: {error}", result.index));
            }
        }
    }
    for index in retry {
        match generate_one(index).await {
            Ok(image) => images.push(image),
            Err(error) => {
                tracing::warn!(index = index + 1, seed = seed + i64::from(index), %error, "copy failed");
                errors.push(format!("copy {index}: {error}"));
            }
        }
    }

    if images.is_empty() {
        return Err(GeminiError::AllCopiesFailed {
            errors: errors.join("; "),
        }
        .into());
    }
    images.sort_by_key(|image| image.index);
    Ok(images)
}

fn generated(index: u32, seed: i64, bytes: Vec<u8>) -> Generated {
    Generated {
        index,
        bytes,
        copy: CopyParams {
            seed: seed + i64::from(index),
            params: UsedParams::default(),
        },
    }
}

/// Generates `copies` images of `prompt`, seeded `seed..seed+copies`, against a
/// `sd-server` process matching `args`'s model flags — spawning or reusing a
/// daemon as needed. Requests are sequential, not one server-side batch: the
/// model stays resident across all of them (the point of the daemon), while
/// each copy's progress and failures stay independently visible. Copy 0 uses
/// `args`'s params exactly; later copies also get steps/cfg_scale/guidance
/// jittered by `args.jitter` (see `jitter::params_for`).
async fn generate_sd(
    args: &GenerateArgs,
    prompt: &str,
    seed: i64,
    config: &Config,
    copies: u32,
) -> Result<Vec<Generated>, AppError> {
    let model_set = build_model_set(args, config)?;
    let client = daemon::ensure_ready(&config.sd_server, &config.log_dir, &model_set).await?;
    let log_path = daemon::log_path(&config.log_dir);
    let jitter_amount = args.jitter.unwrap_or(jitter::DEFAULT_JITTER);

    let mut images = Vec::with_capacity(copies as usize);
    for index in 0..copies {
        let copy = CopyParams {
            seed: seed + i64::from(index),
            params: jitter::params_for(args, seed, index, jitter_amount),
        };
        let request = build_request(args, prompt, &copy)?;

        // `--json` writes the report to stdout; the bar lives on stderr, so
        // nothing would corrupt, but suppressing it keeps a scripted
        // invocation's stderr free of progress-bar control codes too.
        let tailer = if args.json {
            None
        } else {
            let offset = progress::current_offset(&log_path).await;
            Some(progress::spawn(log_path.clone(), offset, config.sd_server.poll_interval()))
        };

        let outcome = run_one(&client, &request, config).await;
        if let Some(tailer) = tailer {
            tailer.stop().await;
        }
        let mut result = outcome?;

        if result.len() != 1 {
            return Err(SdError::ImageCountMismatch {
                expected: 1,
                found: result.len(),
            }
            .into());
        }
        tracing::info!(
            index = index + 1,
            copies,
            seed = copy.seed,
            steps = ?copy.params.steps,
            cfg_scale = ?copy.params.cfg_scale,
            guidance = ?copy.params.guidance,
            "generated image"
        );
        images.push(Generated {
            index,
            bytes: result.remove(0),
            copy,
        });
    }
    Ok(images)
}

async fn run_one(
    client: &crate::sd::client::Client,
    request: &ImgGenRequest,
    config: &Config,
) -> Result<Vec<Vec<u8>>, SdError> {
    let submitted = client.submit_img_gen(request).await?;
    client
        .wait_for_images(
            &submitted.id,
            config.sd_server.poll_interval(),
            config.sd_server.request_timeout(),
        )
        .await
}

fn build_model_set(args: &GenerateArgs, config: &Config) -> Result<ModelSet, AppError> {
    let resolve = |reference: &Option<ModelRef>| -> Result<Option<PathBuf>, AppError> {
        let Some(reference) = reference else { return Ok(None) };
        let path = models::resolve(&config.models_dir, reference)?;
        Ok(Some(path.canonicalize().unwrap_or(path)))
    };

    Ok(ModelSet {
        model: resolve(&args.model)?,
        diffusion_model: resolve(&args.diffusion_model)?,
        vae: resolve(&args.vae)?,
        clip_l: resolve(&args.clip_l)?,
        clip_g: resolve(&args.clip_g)?,
        t5xxl: resolve(&args.t5xxl)?,
        taesd: resolve(&args.taesd)?,
        text_encoder: resolve(&args.text_encoder)?,
        vision_encoder: resolve(&args.vision_encoder)?,
        weight_type: args.weight_type,
        threads: args.threads,
        backend: args.backend,
        vae_tiling: args.vae_tiling.unwrap_or(false),
        flash_attn: args.flash_attn.unwrap_or(false),
        release_tag: config.sd_server.release_tag.clone(),
    })
}

fn build_request(args: &GenerateArgs, prompt: &str, copy: &CopyParams) -> Result<ImgGenRequest, AppError> {
    let mut ref_images = Vec::with_capacity(args.ref_image.len());
    for path in &args.ref_image {
        let bytes = std::fs::read(path)?;
        ref_images.push(BASE64.encode(bytes));
    }

    let params = &copy.params;
    let guidance = (params.cfg_scale.is_some() || params.guidance.is_some()).then_some(Guidance {
        txt_cfg: params.cfg_scale,
        distilled_guidance: params.guidance,
    });
    let sample_params = (params.steps.is_some() || args.sampler.is_some() || guidance.is_some()).then(|| SampleParams {
        sample_method: args.sampler.map(|sampler| server_sampler_name(sampler).to_owned()),
        sample_steps: params.steps,
        guidance,
    });

    Ok(ImgGenRequest {
        prompt: prompt.to_owned(),
        negative_prompt: args.negative.clone(),
        clip_skip: args.clip_skip.map(i32::from),
        width: args.width,
        height: args.height,
        seed: Some(copy.seed),
        batch_count: 1,
        ref_images,
        sample_params,
        output_format: None,
        embed_image_metadata: true,
    })
}

/// `sd-server`'s native sampler name for each `--sampler` value, verified
/// against `sample_method_to_str` in stable-diffusion.cpp's own source — these
/// use underscores and literal `++`, unlike this project's hyphenated CLI
/// spellings, so the two must be mapped explicitly rather than reused.
fn server_sampler_name(sampler: Sampler) -> &'static str {
    match sampler {
        Sampler::Euler => "euler",
        Sampler::EulerA => "euler_a",
        Sampler::Heun => "heun",
        Sampler::Dpm2 => "dpm2",
        Sampler::Dpmpp2sA => "dpm++2s_a",
        Sampler::Dpmpp2m => "dpm++2m",
        Sampler::Dpmpp2mV2 => "dpm++2mv2",
        Sampler::Ipndm => "ipndm",
        Sampler::IpndmV => "ipndm_v",
        Sampler::Lcm => "lcm",
        Sampler::DdimTrailing => "ddim_trailing",
        Sampler::Tcd => "tcd",
        Sampler::ResMultistep => "res_multistep",
        Sampler::Res2s => "res_2s",
        Sampler::ErSde => "er_sde",
        Sampler::EulerCfgPp => "euler_cfg_pp",
        Sampler::EulerACfgPp => "euler_a_cfg_pp",
        Sampler::EulerGe => "euler_ge",
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::cli::{Cli, Command};

    fn parse(args: &[&str]) -> GenerateArgs {
        let mut full = vec!["image-gen", "generate"];
        full.extend_from_slice(args);
        match Cli::try_parse_from(full).unwrap().command {
            Command::Generate(args) => *args,
            other => panic!("expected Command::Generate, got {other:?}"),
        }
    }

    /// Copy 0's params for `args`: today's exact behavior, jitter never applied.
    fn copy0(args: &GenerateArgs, seed: i64) -> CopyParams {
        CopyParams {
            seed,
            params: jitter::params_for(args, seed, 0, jitter::DEFAULT_JITTER),
        }
    }

    #[test]
    fn request_omits_sample_params_when_nothing_was_set() {
        let args = parse(&["a prompt"]);
        let request = build_request(&args, "a prompt", &copy0(&args, 42)).unwrap();
        assert!(request.sample_params.is_none());
        assert_eq!(request.seed, Some(42));
        assert_eq!(request.batch_count, 1);
        assert!(request.embed_image_metadata);
    }

    #[test]
    fn request_carries_steps_and_sampler() {
        let args = parse(&["a prompt", "--steps", "12", "--sampler", "dpmpp2s-a"]);
        let request = build_request(&args, "a prompt", &copy0(&args, 1)).unwrap();
        let sample_params = request.sample_params.unwrap();
        assert_eq!(sample_params.sample_steps, Some(12));
        assert_eq!(sample_params.sample_method.as_deref(), Some("dpm++2s_a"));
    }

    #[test]
    fn request_carries_cfg_and_guidance_scale() {
        let args = parse(&["a prompt", "--cfg-scale", "6.0", "--guidance", "3.5"]);
        let request = build_request(&args, "a prompt", &copy0(&args, 1)).unwrap();
        let guidance = request.sample_params.unwrap().guidance.unwrap();
        assert_eq!(guidance.txt_cfg, Some(6.0));
        assert_eq!(guidance.distilled_guidance, Some(3.5));
    }

    #[test]
    fn jittered_copy_carries_materialized_sample_params() {
        let args = parse(&["a prompt"]);
        let copy = CopyParams {
            seed: 1,
            params: jitter::params_for(&args, 42, 1, 0.12),
        };
        let request = build_request(&args, "a prompt", &copy).unwrap();
        let sample_params = request.sample_params.unwrap();
        assert!(sample_params.sample_steps.is_some());
        let guidance = sample_params.guidance.unwrap();
        assert!(guidance.txt_cfg.is_some());
        assert!(guidance.distilled_guidance.is_some());
    }

    #[test]
    fn every_sampler_variant_maps_to_a_non_empty_native_name() {
        for sampler in [
            Sampler::Euler,
            Sampler::EulerA,
            Sampler::Heun,
            Sampler::Dpm2,
            Sampler::Dpmpp2sA,
            Sampler::Dpmpp2m,
            Sampler::Dpmpp2mV2,
            Sampler::Ipndm,
            Sampler::IpndmV,
            Sampler::Lcm,
            Sampler::DdimTrailing,
            Sampler::Tcd,
            Sampler::ResMultistep,
            Sampler::Res2s,
            Sampler::ErSde,
            Sampler::EulerCfgPp,
            Sampler::EulerACfgPp,
            Sampler::EulerGe,
        ] {
            assert!(!server_sampler_name(sampler).is_empty());
        }
    }

    #[test]
    fn model_set_carries_text_and_vision_encoder() {
        let dir = tempfile::tempdir().unwrap();
        let diffusion_model = dir.path().join("qwen_image_2.1.gguf");
        let text_encoder = dir.path().join("qwen3-vl-8b.gguf");
        let vision_encoder = dir.path().join("mmproj.gguf");
        for path in [&diffusion_model, &text_encoder, &vision_encoder] {
            std::fs::write(path, b"fake").unwrap();
        }

        let mut args = parse(&["a prompt"]);
        args.diffusion_model = Some(ModelRef::Local(diffusion_model));
        args.text_encoder = Some(ModelRef::Local(text_encoder.clone()));
        args.vision_encoder = Some(ModelRef::Local(vision_encoder.clone()));

        let config = Config {
            models_dir: dir.path().to_path_buf(),
            ..Config::default()
        };

        let model_set = build_model_set(&args, &config).unwrap();
        assert_eq!(model_set.text_encoder, Some(text_encoder.canonicalize().unwrap()));
        assert_eq!(model_set.vision_encoder, Some(vision_encoder.canonicalize().unwrap()));
    }

    #[test]
    fn ref_images_are_base64_encoded_file_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ref.png");
        std::fs::write(&path, b"fake png bytes").unwrap();

        let mut args = parse(&["a prompt"]);
        args.ref_image = vec![path];
        let request = build_request(&args, "a prompt", &copy0(&args, 1)).unwrap();

        assert_eq!(request.ref_images, vec![BASE64.encode(b"fake png bytes")]);
    }

    mod gemini_batch {
        use super::*;
        use crate::gemini::GeminiConfig;
        use serde_json::{Value, json};
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        const GENERATE_PATH: &str = "/models/gemini-2.5-flash-image:generateContent";
        const BATCH_PATH: &str = "/models/gemini-2.5-flash-image:batchGenerateContent";

        fn config_for(server: &MockServer) -> Config {
            Config {
                gemini: GeminiConfig {
                    api_key: Some("k".to_owned()),
                    base_url: server.uri(),
                    timeout_secs: 5,
                    batch_poll_secs: 1,
                },
                ..Config::default()
            }
        }

        fn image(bytes: &str) -> Value {
            json!({"candidates": [{
                "content": {"parts": [{"inlineData": {
                    "mimeType": "image/png",
                    "data": BASE64.encode(bytes),
                }}]},
                "finishReason": "STOP"
            }]})
        }

        async fn mount_create(server: &MockServer) {
            Mock::given(method("POST"))
                .and(path(BATCH_PATH))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({"name": "batches/1"})))
                .expect(1)
                .mount(server)
                .await;
        }

        async fn mount_poll(server: &MockServer, body: Value) {
            Mock::given(method("GET"))
                .and(path("/batches/1"))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(server)
                .await;
        }

        async fn mount_cancel(server: &MockServer) {
            Mock::given(method("POST"))
                .and(path("/batches/1:cancel"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
                .expect(1)
                .mount(server)
                .await;
        }

        async fn mount_interactive(server: &MockServer, calls: u64) {
            Mock::given(method("POST"))
                .and(path(GENERATE_PATH))
                .respond_with(ResponseTemplate::new(200).set_body_json(image("interactive")))
                .expect(calls)
                .mount(server)
                .await;
        }

        #[tokio::test]
        async fn failed_copies_are_skipped_and_indices_are_kept() {
            let server = MockServer::start().await;
            mount_create(&server).await;
            mount_poll(
                &server,
                json!({"state": "JOB_STATE_SUCCEEDED", "dest": {"inlinedResponses": [
                    {"metadata": {"key": "0"}, "response": image("a")},
                    {"metadata": {"key": "1"}, "error": {"message": "blocked"}},
                    {"metadata": {"key": "2"}, "response": image("c")},
                ]}}),
            )
            .await;
            mount_interactive(&server, 0).await;

            let args = parse(&["p", "--provider", "gemini", "--batch", "--copies", "3", "--seed", "10"]);
            let images = generate(&args, "p", 10, &config_for(&server), 3).await.unwrap();

            assert_eq!(images.iter().map(|i| i.index).collect::<Vec<_>>(), vec![0, 2]);
            assert_eq!(images[1].copy.seed, 12);
            assert_eq!(images[1].bytes, b"c");
        }

        #[tokio::test]
        async fn every_copy_failing_is_an_error() {
            let server = MockServer::start().await;
            mount_create(&server).await;
            mount_poll(
                &server,
                json!({"state": "JOB_STATE_SUCCEEDED", "dest": {"inlinedResponses": [
                    {"error": {"message": "blocked"}},
                ]}}),
            )
            .await;

            let args = parse(&["p", "--provider", "gemini", "--batch"]);
            let err = generate(&args, "p", 1, &config_for(&server), 1).await.unwrap_err();
            assert!(matches!(err, AppError::Gemini(GeminiError::AllCopiesFailed { .. })), "{err:?}");
        }

        #[tokio::test]
        async fn timeout_falls_back_to_interactive_for_only_the_missing_copies() {
            let server = MockServer::start().await;
            mount_create(&server).await;
            mount_cancel(&server).await;
            mount_poll(
                &server,
                json!({"state": "JOB_STATE_RUNNING", "dest": {"inlinedResponses": [
                    {"metadata": {"key": "1"}, "response": image("batched")},
                ]}}),
            )
            .await;
            mount_interactive(&server, 2).await;

            let args = parse(&["p", "--provider", "gemini", "--batch", "--batch-wait", "1s"]);
            let images = generate(&args, "p", 0, &config_for(&server), 3).await.unwrap();

            assert_eq!(images.iter().map(|i| i.index).collect::<Vec<_>>(), vec![0, 1, 2]);
            assert_eq!(images[1].bytes, b"batched");
            assert_eq!(images[0].bytes, b"interactive");
        }

        #[tokio::test]
        async fn timeout_with_fail_fallback_is_an_error() {
            let server = MockServer::start().await;
            mount_create(&server).await;
            mount_cancel(&server).await;
            mount_poll(&server, json!({"state": "JOB_STATE_RUNNING"})).await;
            mount_interactive(&server, 0).await;

            let args = parse(&[
                "p", "--provider", "gemini", "--batch", "--batch-wait", "1s", "--batch-fallback", "fail",
            ]);
            let err = generate(&args, "p", 0, &config_for(&server), 2).await.unwrap_err();
            assert!(matches!(err, AppError::Gemini(GeminiError::BatchTimedOut { .. })), "{err:?}");
        }

        #[tokio::test]
        async fn without_batch_copies_are_interactive_and_fail_fast() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path(GENERATE_PATH))
                .respond_with(ResponseTemplate::new(500).set_body_string("down"))
                .expect(1)
                .mount(&server)
                .await;

            let args = parse(&["p", "--provider", "gemini"]);
            let err = generate(&args, "p", 0, &config_for(&server), 3).await.unwrap_err();
            assert!(matches!(err, AppError::Gemini(GeminiError::Status { .. })), "{err:?}");
        }
    }
}
