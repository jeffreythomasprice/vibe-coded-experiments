use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use diffusion_rs::api::{
    BackendDevice, ClipSkip, ConfigBuilder, ModelConfigBuilder, Module, gen_img,
};

use crate::cli::GenerateArgs;
use crate::error::AppError;
use crate::models;

pub fn generate(
    args: &GenerateArgs,
    models_dir: &Path,
    output: &Path,
    copies: u32,
) -> Result<(), AppError> {
    let mut model_config = ModelConfigBuilder::default();

    if let Some(reference) = &args.model {
        model_config.model(models::resolve(models_dir, reference)?);
    }
    if let Some(reference) = &args.diffusion_model {
        model_config.diffusion_model(models::resolve(models_dir, reference)?);
    }
    if let Some(reference) = &args.vae {
        model_config.vae(models::resolve(models_dir, reference)?);
    }
    if let Some(reference) = &args.clip_l {
        model_config.clip_l(models::resolve(models_dir, reference)?);
    }
    if let Some(reference) = &args.clip_g {
        model_config.clip_g(models::resolve(models_dir, reference)?);
    }
    if let Some(reference) = &args.t5xxl {
        model_config.t5xxl(models::resolve(models_dir, reference)?);
    }
    if let Some(reference) = &args.taesd {
        model_config.taesd(models::resolve(models_dir, reference)?);
    }
    if let Some(weight_type) = args.weight_type {
        model_config.weight_type(weight_type);
    }
    model_config
        .vae_tiling(args.vae_tiling.unwrap_or(false))
        .flash_attention(args.flash_attn.unwrap_or(false));
    if let Some(threads) = args.threads {
        model_config.n_threads(threads);
    }
    if let Some(backend) = args.backend {
        if let Some(feature) = backend.missing_feature() {
            return Err(AppError::UnsupportedBackend { backend: feature });
        }
        tracing::info!(backend = backend.as_str(), "using backend");
        let device = BackendDevice::from(backend);
        model_config.backend(HashMap::from([
            (Module::Diffusion, device.clone()),
            (Module::Te, device.clone()),
            (Module::ClipVision, device.clone()),
            (Module::Vae, device.clone()),
            (Module::Controlnet, device.clone()),
            (Module::Photomaker, device.clone()),
            (Module::Upscaler, device),
        ]));
    } else {
        tracing::info!("backend: auto-detecting best available GPU, falling back to CPU");
    }

    let mut gen_config = ConfigBuilder::default();
    gen_config.prompt(args.prompt.clone()).output(output);
    if let Some(negative) = &args.negative {
        gen_config.negative_prompt(negative.clone());
    }
    if let Some(width) = args.width {
        gen_config.width(width);
    }
    if let Some(height) = args.height {
        gen_config.height(height);
    }
    if let Some(steps) = args.steps {
        gen_config.steps(steps);
    }
    if let Some(cfg_scale) = args.cfg_scale {
        gen_config.cfg_scale(cfg_scale);
    }
    if let Some(guidance) = args.guidance {
        gen_config.guidance(guidance);
    }
    if let Some(seed) = args.seed {
        gen_config.seed(seed);
    }
    if let Some(sampler) = args.sampler {
        gen_config.sampling_method(sampler);
    }
    if let Some(clip_skip) = args.clip_skip {
        gen_config.clip_skip(match clip_skip {
            1 => ClipSkip::None,
            2 => ClipSkip::OneLayer,
            _ => ClipSkip::Unspecified,
        });
    }
    if copies > 1 {
        gen_config.batch_count(copies as i32);
    }

    let gen_config = gen_config.build()?;
    let mut model_config = model_config.build()?;

    let started = Instant::now();
    gen_img(&gen_config, &mut model_config)?;
    tracing::info!(elapsed = ?started.elapsed(), copies, "generated image");

    Ok(())
}
