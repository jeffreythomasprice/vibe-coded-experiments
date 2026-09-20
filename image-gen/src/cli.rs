use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use diffusion_rs::api::{BackendDevice, SampleMethod, WeightType};

use crate::models::ModelRef;

#[derive(Debug, Parser)]
#[command(name = "image-gen", version, about = "Generate images with diffusion-rs")]
pub struct Cli {
    /// Path to a config.toml, overriding the default search locations
    #[arg(short, long, value_name = "PATH", global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Generate an image from a prompt
    Generate(Box<GenerateArgs>),
    /// Find models to pull, and see which ones you already have
    Models {
        #[command(subcommand)]
        command: ModelsCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum ModelsCommand {
    /// Search HuggingFace for models you could pull
    Search(SearchArgs),
    /// List weight files already downloaded into models_dir
    List(ListArgs),
}

#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Text to search HuggingFace model names for
    pub query: String,

    /// Maximum number of repos to inspect
    #[arg(
        long,
        value_name = "N",
        default_value_t = 20,
        value_parser = clap::value_parser!(u32).range(1..=100)
    )]
    pub limit: u32,

    /// Include every pipeline type, not just text-to-image
    #[arg(long)]
    pub all: bool,

    /// Ignore cached hub responses and re-query HuggingFace
    #[arg(long)]
    pub refresh: bool,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Only show refs containing this text
    pub query: Option<String>,
}

#[derive(Debug, Args)]
pub struct GenerateArgs {
    /// The prompt to render
    pub prompt: String,

    /// Apply a named preset from config.toml; repeatable, combined left to right
    /// (later presets win on collision); explicit flags still win over any preset
    #[arg(long, value_name = "NAME")]
    pub preset: Vec<String>,

    /// Full checkpoint: owner/repo[:file], owner/repo@revision[:file], or a local path
    #[arg(short, long, value_name = "REF")]
    pub model: Option<ModelRef>,

    /// Standalone diffusion model (Flux/SD3-style); same REF syntax as --model
    #[arg(long, value_name = "REF")]
    pub diffusion_model: Option<ModelRef>,

    /// VAE; same REF syntax as --model
    #[arg(long, value_name = "REF")]
    pub vae: Option<ModelRef>,

    /// CLIP-L text encoder; same REF syntax as --model
    #[arg(long, value_name = "REF")]
    pub clip_l: Option<ModelRef>,

    /// CLIP-G text encoder; same REF syntax as --model
    #[arg(long, value_name = "REF")]
    pub clip_g: Option<ModelRef>,

    /// T5-XXL text encoder; same REF syntax as --model
    #[arg(long, value_name = "REF")]
    pub t5xxl: Option<ModelRef>,

    /// Tiny AutoEncoder for fast, low-quality decoding; same REF syntax as --model
    #[arg(long, value_name = "REF")]
    pub taesd: Option<ModelRef>,

    /// Weight precision override (default: the type stored in the weight file)
    #[arg(long, value_name = "TYPE")]
    pub weight_type: Option<WeightTypeArg>,

    /// Process the VAE in tiles to reduce memory usage (default: false)
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        default_missing_value = "true",
        require_equals = true
    )]
    pub vae_tiling: Option<bool>,

    /// Use flash attention to reduce memory usage (default: false)
    #[arg(
        long,
        value_name = "BOOL",
        num_args = 0..=1,
        default_missing_value = "true",
        require_equals = true
    )]
    pub flash_attn: Option<bool>,

    /// Number of CPU threads to use (default: physical core count)
    #[arg(long, value_name = "N")]
    pub threads: Option<i32>,

    /// Compute backend to run on (default: best available GPU, falling back to CPU)
    #[arg(long, value_name = "BACKEND")]
    pub backend: Option<Backend>,

    /// Negative prompt (default: "")
    #[arg(short = 'n', long, value_name = "TEXT")]
    pub negative: Option<String>,

    /// Image width, in pixels (default: 512)
    #[arg(short = 'W', long, value_name = "PX")]
    pub width: Option<i32>,

    /// Image height, in pixels (default: 512)
    #[arg(short = 'H', long, value_name = "PX")]
    pub height: Option<i32>,

    /// Number of sampling steps (default: 20)
    #[arg(short, long, value_name = "N")]
    pub steps: Option<i32>,

    /// Unconditional guidance scale (default: 7.0)
    #[arg(long, value_name = "F")]
    pub cfg_scale: Option<f32>,

    /// Distilled guidance scale for models with guidance input (default: 3.5)
    #[arg(long, value_name = "F")]
    pub guidance: Option<f32>,

    /// RNG seed (default: random)
    #[arg(long, value_name = "N")]
    pub seed: Option<i64>,

    /// Sampling method (default: chosen by the backend)
    #[arg(long, value_name = "NAME")]
    pub sampler: Option<Sampler>,

    /// Ignore last layers of CLIP: 1 ignores none, 2 ignores one layer (default: unspecified)
    #[arg(long, value_name = "0|1|2", value_parser = clap::value_parser!(u8).range(0..=2))]
    pub clip_skip: Option<u8>,

    /// Write the generated image here
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Generate this many images; with -o the path becomes a filename prefix
    #[arg(
        long,
        value_name = "N",
        default_value_t = 1,
        value_parser = clap::value_parser!(u32).range(1..)
    )]
    pub copies: u32,

    /// Display the generated image inline in the terminal; with --eval, ordered
    /// worst-scoring first so the best is last
    #[arg(long)]
    pub show: bool,

    /// Print the prompt, each image's path/seed/scores, and total elapsed time as
    /// JSON on stdout
    #[arg(long)]
    pub json: bool,

    /// Score each generated image against the prompt; repeatable (an inclusive or)
    #[arg(long = "eval", value_name = "METRIC")]
    pub eval: Option<Vec<EvalMetric>>,

    /// When to compress the prompt with an LLM before generating (default: auto)
    #[arg(long, value_name = "MODE")]
    pub rewrite: Option<RewriteMode>,

    /// Prompt length, in characters, above which `--rewrite auto` fires (default: 300)
    #[arg(long, value_name = "N")]
    pub rewrite_threshold: Option<usize>,

    /// Model used to rewrite the prompt (default: [llm].model)
    #[arg(long, value_name = "MODEL")]
    pub rewrite_model: Option<String>,

    /// Vision model used for VQAScore (default: [llm].model)
    #[arg(long, value_name = "MODEL")]
    pub vqa_model: Option<String>,

    /// Vision model used to caption images for TIT-Score (default: [llm].model)
    #[arg(long, value_name = "MODEL")]
    pub caption_model: Option<String>,

    /// Text model used to judge TIT-Score captions against the prompt (default: [llm].model)
    #[arg(long, value_name = "MODEL")]
    pub judge_model: Option<String>,

    /// Downscale images to at most this many pixels per side before sending to an
    /// eval model (default: 512)
    #[arg(long, value_name = "PX")]
    pub eval_max_px: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum EvalMetric {
    Vqa,
    Tit,
}

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum RewriteMode {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum Backend {
    Cpu,
    Vulkan,
    Cuda,
}

impl From<Backend> for BackendDevice {
    fn from(value: Backend) -> Self {
        match value {
            Backend::Cpu => BackendDevice::CPU,
            Backend::Vulkan => BackendDevice::VULKAN0,
            Backend::Cuda => BackendDevice::CUDA0,
        }
    }
}

impl Backend {
    /// The `--features` name needed to compile this backend in, or `None` if it's
    /// always available (cpu) or was compiled in for this binary.
    pub fn missing_feature(self) -> Option<&'static str> {
        match self {
            Backend::Cpu => None,
            Backend::Vulkan if cfg!(feature = "vulkan") => None,
            Backend::Vulkan => Some("vulkan"),
            Backend::Cuda if cfg!(feature = "cuda") => None,
            Backend::Cuda => Some("cuda"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Cpu => "cpu",
            Backend::Vulkan => "vulkan",
            Backend::Cuda => "cuda",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum Sampler {
    #[value(name = "euler")]
    Euler,
    #[value(name = "euler-a")]
    EulerA,
    #[value(name = "heun")]
    Heun,
    #[value(name = "dpm2")]
    Dpm2,
    #[value(name = "dpmpp2s-a")]
    Dpmpp2sA,
    #[value(name = "dpmpp2m")]
    Dpmpp2m,
    #[value(name = "dpmpp2mv2")]
    Dpmpp2mV2,
    #[value(name = "ipndm")]
    Ipndm,
    #[value(name = "ipndm-v")]
    IpndmV,
    #[value(name = "lcm")]
    Lcm,
    #[value(name = "ddim-trailing")]
    DdimTrailing,
    #[value(name = "tcd")]
    Tcd,
    #[value(name = "res-multistep")]
    ResMultistep,
    #[value(name = "res-2s")]
    Res2s,
    #[value(name = "er-sde")]
    ErSde,
    #[value(name = "euler-cfg-pp")]
    EulerCfgPp,
    #[value(name = "euler-a-cfg-pp")]
    EulerACfgPp,
    #[value(name = "euler-ge")]
    EulerGe,
}

impl From<Sampler> for SampleMethod {
    fn from(value: Sampler) -> Self {
        match value {
            Sampler::Euler => SampleMethod::EULER_SAMPLE_METHOD,
            Sampler::EulerA => SampleMethod::EULER_A_SAMPLE_METHOD,
            Sampler::Heun => SampleMethod::HEUN_SAMPLE_METHOD,
            Sampler::Dpm2 => SampleMethod::DPM2_SAMPLE_METHOD,
            Sampler::Dpmpp2sA => SampleMethod::DPMPP2S_A_SAMPLE_METHOD,
            Sampler::Dpmpp2m => SampleMethod::DPMPP2M_SAMPLE_METHOD,
            Sampler::Dpmpp2mV2 => SampleMethod::DPMPP2Mv2_SAMPLE_METHOD,
            Sampler::Ipndm => SampleMethod::IPNDM_SAMPLE_METHOD,
            Sampler::IpndmV => SampleMethod::IPNDM_V_SAMPLE_METHOD,
            Sampler::Lcm => SampleMethod::LCM_SAMPLE_METHOD,
            Sampler::DdimTrailing => SampleMethod::DDIM_TRAILING_SAMPLE_METHOD,
            Sampler::Tcd => SampleMethod::TCD_SAMPLE_METHOD,
            Sampler::ResMultistep => SampleMethod::RES_MULTISTEP_SAMPLE_METHOD,
            Sampler::Res2s => SampleMethod::RES_2S_SAMPLE_METHOD,
            Sampler::ErSde => SampleMethod::ER_SDE_SAMPLE_METHOD,
            Sampler::EulerCfgPp => SampleMethod::EULER_CFG_PP_SAMPLE_METHOD,
            Sampler::EulerACfgPp => SampleMethod::EULER_A_CFG_PP_SAMPLE_METHOD,
            Sampler::EulerGe => SampleMethod::EULER_GE_SAMPLE_METHOD,
        }
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum WeightTypeArg {
    #[value(name = "f32")]
    F32,
    #[value(name = "f16")]
    F16,
    #[value(name = "bf16")]
    Bf16,
    #[value(name = "q8_0")]
    Q8_0,
    #[value(name = "q5_1")]
    Q5_1,
    #[value(name = "q5_0")]
    Q5_0,
    #[value(name = "q4_1")]
    Q4_1,
    #[value(name = "q4_0")]
    Q4_0,
    #[value(name = "q6_k")]
    Q6K,
    #[value(name = "q5_k")]
    Q5K,
    #[value(name = "q4_k")]
    Q4K,
    #[value(name = "q3_k")]
    Q3K,
    #[value(name = "q2_k")]
    Q2K,
}

impl From<WeightTypeArg> for WeightType {
    fn from(value: WeightTypeArg) -> Self {
        match value {
            WeightTypeArg::F32 => WeightType::SD_TYPE_F32,
            WeightTypeArg::F16 => WeightType::SD_TYPE_F16,
            WeightTypeArg::Bf16 => WeightType::SD_TYPE_BF16,
            WeightTypeArg::Q8_0 => WeightType::SD_TYPE_Q8_0,
            WeightTypeArg::Q5_1 => WeightType::SD_TYPE_Q5_1,
            WeightTypeArg::Q5_0 => WeightType::SD_TYPE_Q5_0,
            WeightTypeArg::Q4_1 => WeightType::SD_TYPE_Q4_1,
            WeightTypeArg::Q4_0 => WeightType::SD_TYPE_Q4_0,
            WeightTypeArg::Q6K => WeightType::SD_TYPE_Q6_K,
            WeightTypeArg::Q5K => WeightType::SD_TYPE_Q5_K,
            WeightTypeArg::Q4K => WeightType::SD_TYPE_Q4_K,
            WeightTypeArg::Q3K => WeightType::SD_TYPE_Q3_K,
            WeightTypeArg::Q2K => WeightType::SD_TYPE_Q2_K,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use super::{Cli, Command};

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn generate_accepts_json_flag() {
        let cli = Cli::try_parse_from(["image-gen", "generate", "a prompt", "--json"]).unwrap();
        match cli.command {
            Command::Generate(args) => assert!(args.json),
            other => panic!("expected Command::Generate, got {other:?}"),
        }
    }

    #[test]
    fn models_list_rejects_json_flag() {
        assert!(Cli::try_parse_from(["image-gen", "models", "list", "--json"]).is_err());
    }

    #[test]
    fn eval_absent_is_none_not_empty_vec() {
        let cli = Cli::try_parse_from(["image-gen", "generate", "a prompt"]).unwrap();
        match cli.command {
            Command::Generate(args) => assert_eq!(args.eval, None),
            other => panic!("expected Command::Generate, got {other:?}"),
        }
    }

    #[test]
    fn repeated_eval_flags_combine_into_one_vec() {
        let cli = Cli::try_parse_from([
            "image-gen", "generate", "a prompt", "--eval", "vqa", "--eval", "tit",
        ])
        .unwrap();
        match cli.command {
            Command::Generate(args) => {
                assert_eq!(args.eval, Some(vec![super::EvalMetric::Vqa, super::EvalMetric::Tit]));
            }
            other => panic!("expected Command::Generate, got {other:?}"),
        }
    }

    #[test]
    fn repeated_preset_flags_collect_in_order() {
        let cli = Cli::try_parse_from([
            "image-gen", "generate", "a prompt", "--preset", "a", "--preset", "b",
        ])
        .unwrap();
        match cli.command {
            Command::Generate(args) => assert_eq!(args.preset, vec!["a", "b"]),
            other => panic!("expected Command::Generate, got {other:?}"),
        }
    }
}
