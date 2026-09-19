use diffusion_rs::api::{ConfigBuilder, ModelConfigBuilder, gen_img};

use crate::config::Config;
use crate::error::AppError;
use crate::models;

/// Generates an image from `prompt` using SDXL Turbo, downloading model
/// weights into `config.models_dir` on first use.
///
/// Not yet wired up to the CLI.
#[allow(dead_code)]
pub fn generate(config: &Config, prompt: &str) -> Result<(), AppError> {
    let model = models::download(
        &config.models_dir,
        "stabilityai/sdxl-turbo",
        "sd_xl_turbo_1.0_fp16.safetensors",
    )?;
    let vae = models::download(
        &config.models_dir,
        "madebyollin/sdxl-vae-fp16-fix",
        "sdxl.vae.safetensors",
    )?;

    let mut model_config = ModelConfigBuilder::default();
    model_config.model(model).vae(vae);

    let mut gen_config = ConfigBuilder::default();
    gen_config
        .guidance(0f32)
        .cfg_scale(1f32)
        .steps(4)
        .prompt(prompt);

    let gen_config = gen_config.build()?;
    let mut model_config = model_config.build()?;
    gen_img(&gen_config, &mut model_config)?;

    Ok(())
}
