use std::path::PathBuf;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::cli::{GenerateArgs, Sampler};
use crate::config::Config;
use crate::error::AppError;
use crate::models::{self, ModelRef};
use crate::sd::client::{Guidance, ImgGenRequest, SampleParams};
use crate::sd::modelset::ModelSet;
use crate::sd::{SdError, daemon, progress};

/// Generates `copies` images of `prompt`, seeded `seed..seed+copies`, against a
/// `sd-server` process matching `args`'s model flags — spawning or reusing a
/// daemon as needed. Requests are sequential, not one server-side batch: the
/// model stays resident across all of them (the point of the daemon), while
/// each copy's progress and failures stay independently visible.
pub async fn generate(
    args: &GenerateArgs,
    prompt: &str,
    seed: i64,
    config: &Config,
    copies: u32,
) -> Result<Vec<Vec<u8>>, AppError> {
    let model_set = build_model_set(args, config)?;
    let client = daemon::ensure_ready(&config.sd_server, &config.log_dir, &model_set).await?;
    let log_path = daemon::log_path(&config.log_dir);

    let mut images = Vec::with_capacity(copies as usize);
    for index in 0..copies {
        let request = build_request(args, prompt, seed + i64::from(index))?;

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
        images.push(result.remove(0));
        tracing::info!(index = index + 1, copies, "generated image");
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

fn build_request(args: &GenerateArgs, prompt: &str, seed: i64) -> Result<ImgGenRequest, AppError> {
    let mut ref_images = Vec::with_capacity(args.ref_image.len());
    for path in &args.ref_image {
        let bytes = std::fs::read(path)?;
        ref_images.push(BASE64.encode(bytes));
    }

    let guidance = (args.cfg_scale.is_some() || args.guidance.is_some()).then_some(Guidance {
        txt_cfg: args.cfg_scale,
        distilled_guidance: args.guidance,
    });
    let sample_params = (args.steps.is_some() || args.sampler.is_some() || guidance.is_some()).then(|| SampleParams {
        sample_method: args.sampler.map(|sampler| server_sampler_name(sampler).to_owned()),
        sample_steps: args.steps,
        guidance,
    });

    Ok(ImgGenRequest {
        prompt: prompt.to_owned(),
        negative_prompt: args.negative.clone(),
        clip_skip: args.clip_skip.map(i32::from),
        width: args.width,
        height: args.height,
        seed: Some(seed),
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

    #[test]
    fn request_omits_sample_params_when_nothing_was_set() {
        let args = parse(&["a prompt"]);
        let request = build_request(&args, "a prompt", 42).unwrap();
        assert!(request.sample_params.is_none());
        assert_eq!(request.seed, Some(42));
        assert_eq!(request.batch_count, 1);
        assert!(request.embed_image_metadata);
    }

    #[test]
    fn request_carries_steps_and_sampler() {
        let args = parse(&["a prompt", "--steps", "12", "--sampler", "dpmpp2s-a"]);
        let request = build_request(&args, "a prompt", 1).unwrap();
        let sample_params = request.sample_params.unwrap();
        assert_eq!(sample_params.sample_steps, Some(12));
        assert_eq!(sample_params.sample_method.as_deref(), Some("dpm++2s_a"));
    }

    #[test]
    fn request_carries_cfg_and_guidance_scale() {
        let args = parse(&["a prompt", "--cfg-scale", "6.0", "--guidance", "3.5"]);
        let request = build_request(&args, "a prompt", 1).unwrap();
        let guidance = request.sample_params.unwrap().guidance.unwrap();
        assert_eq!(guidance.txt_cfg, Some(6.0));
        assert_eq!(guidance.distilled_guidance, Some(3.5));
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
        let request = build_request(&args, "a prompt", 1).unwrap();

        assert_eq!(request.ref_images, vec![BASE64.encode(b"fake png bytes")]);
    }
}
