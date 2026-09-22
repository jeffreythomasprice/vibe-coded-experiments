use std::ffi::OsString;
use std::path::PathBuf;

use clap::ValueEnum;
use sha2::{Digest, Sha256};

use crate::cli::{Backend, WeightTypeArg};

/// The full set of startup-time choices that determine which `sd-server` process
/// is needed: every model file it must load, plus every context flag that can
/// only be set at process start (`--backend`, `--type`, thread count, ...).
/// Changing any field here means the daemon must be restarted with a new
/// process, which is why `to_args` is the single place flag names are decided
/// and `fingerprint` is derived from it rather than computed independently —
/// the two can never drift apart.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelSet {
    pub model: Option<PathBuf>,
    pub diffusion_model: Option<PathBuf>,
    pub vae: Option<PathBuf>,
    pub clip_l: Option<PathBuf>,
    pub clip_g: Option<PathBuf>,
    pub t5xxl: Option<PathBuf>,
    pub taesd: Option<PathBuf>,
    /// Qwen-Image-2.1's text encoder; server flag `--llm`. Spelled
    /// `text_encoder` here, not `llm`, to avoid colliding with this project's
    /// own `[llm]` config section and `--*-model` flags, which already mean the
    /// Ollama evaluation model.
    pub text_encoder: Option<PathBuf>,
    /// The matching vision/mmproj weights for image-editing mode; server flag
    /// `--llm_vision`.
    pub vision_encoder: Option<PathBuf>,
    pub weight_type: Option<WeightTypeArg>,
    pub threads: Option<i32>,
    pub backend: Option<Backend>,
    pub vae_tiling: bool,
    pub flash_attn: bool,
    /// The `sd-server` release this process was (or would be) spawned from.
    /// Not itself a CLI argument, but part of the fingerprint: upgrading the
    /// binary must force a restart even if every model path is unchanged.
    pub release_tag: String,
}

impl ModelSet {
    /// The exact argv `sd-server` should be spawned with for these model/context
    /// choices, in a fixed field order so the same `ModelSet` always produces the
    /// same argv (required for `fingerprint` to be stable). Excludes
    /// `--listen-ip`/`--listen-port`/`--log-level`, which are daemon-process
    /// concerns, not model-identity ones.
    pub fn to_args(&self) -> Vec<OsString> {
        let mut args = Vec::new();

        push_path(&mut args, "--model", &self.model);
        push_path(&mut args, "--diffusion-model", &self.diffusion_model);
        push_path(&mut args, "--vae", &self.vae);
        push_path(&mut args, "--clip_l", &self.clip_l);
        push_path(&mut args, "--clip_g", &self.clip_g);
        push_path(&mut args, "--t5xxl", &self.t5xxl);
        push_path(&mut args, "--tae", &self.taesd);
        push_path(&mut args, "--llm", &self.text_encoder);
        push_path(&mut args, "--llm_vision", &self.vision_encoder);

        if let Some(weight_type) = self.weight_type {
            args.push(OsString::from("--type"));
            args.push(OsString::from(value_name(weight_type)));
        }
        if let Some(threads) = self.threads {
            args.push(OsString::from("--threads"));
            args.push(OsString::from(threads.to_string()));
        }
        if let Some(backend) = self.backend {
            args.push(OsString::from("--backend"));
            args.push(OsString::from(backend.as_str()));
        }
        if self.vae_tiling {
            args.push(OsString::from("--vae-tiling"));
        }
        if self.flash_attn {
            args.push(OsString::from("--diffusion-fa"));
        }

        args
    }

    /// A stable identifier for this exact model/context/binary combination.
    /// Two `ModelSet`s with the same fingerprint spawn byte-identical `sd-server`
    /// invocations; any difference (including a `release_tag` bump) changes it.
    pub fn fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.release_tag.as_bytes());
        hasher.update(b"\0");
        for arg in self.to_args() {
            hasher.update(arg.as_encoded_bytes());
            hasher.update(b"\0");
        }
        hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

fn push_path(args: &mut Vec<OsString>, flag: &str, path: &Option<PathBuf>) {
    if let Some(path) = path {
        args.push(OsString::from(flag));
        args.push(path.clone().into_os_string());
    }
}

/// The exact string `sd-server`'s `--type` flag expects, reusing the same
/// spellings `WeightTypeArg` already exposes as its clap `ValueEnum` names
/// (`q8_0`, `bf16`, ...) rather than a second hand-maintained name table.
fn value_name(weight_type: WeightTypeArg) -> String {
    weight_type
        .to_possible_value()
        .expect("WeightTypeArg has no skipped variants")
        .get_name()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> ModelSet {
        ModelSet {
            model: None,
            diffusion_model: Some(PathBuf::from("/models/qwen_image_2.1-Q8_0.gguf")),
            vae: Some(PathBuf::from("/models/qwen_image_2.1_vae_bf16.safetensors")),
            clip_l: None,
            clip_g: None,
            t5xxl: None,
            taesd: None,
            text_encoder: Some(PathBuf::from("/models/Qwen3-VL-8B-Instruct-Q4_K_M.gguf")),
            vision_encoder: None,
            weight_type: None,
            threads: None,
            backend: None,
            vae_tiling: false,
            flash_attn: false,
            release_tag: "master-890-74988b2".to_owned(),
        }
    }

    #[test]
    fn to_args_snapshot_guards_server_flag_names() {
        let args = minimal().to_args();
        let args: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(
            args,
            vec![
                "--diffusion-model",
                "/models/qwen_image_2.1-Q8_0.gguf",
                "--vae",
                "/models/qwen_image_2.1_vae_bf16.safetensors",
                "--llm",
                "/models/Qwen3-VL-8B-Instruct-Q4_K_M.gguf",
            ]
        );
    }

    #[test]
    fn to_args_uses_underscored_clip_and_llm_vision_flags() {
        let mut set = minimal();
        set.clip_l = Some(PathBuf::from("/models/clip_l.safetensors"));
        set.clip_g = Some(PathBuf::from("/models/clip_g.safetensors"));
        set.vision_encoder = Some(PathBuf::from("/models/mmproj.gguf"));
        let args = set.to_args();
        let args: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();
        assert!(args.windows(2).any(|w| w == ["--clip_l", "/models/clip_l.safetensors"]));
        assert!(args.windows(2).any(|w| w == ["--clip_g", "/models/clip_g.safetensors"]));
        assert!(args.windows(2).any(|w| w == ["--llm_vision", "/models/mmproj.gguf"]));
    }

    #[test]
    fn to_args_uses_tae_not_taesd() {
        let mut set = minimal();
        set.taesd = Some(PathBuf::from("/models/taesd.safetensors"));
        let args = set.to_args();
        let args: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();
        assert!(args.windows(2).any(|w| w == ["--tae", "/models/taesd.safetensors"]));
        assert!(!args.contains(&"--taesd"));
    }

    #[test]
    fn to_args_uses_type_not_weight_type() {
        let mut set = minimal();
        set.weight_type = Some(WeightTypeArg::Q8_0);
        let args = set.to_args();
        let args: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();
        assert!(args.windows(2).any(|w| w == ["--type", "q8_0"]));
    }

    #[test]
    fn to_args_uses_diffusion_fa_not_flash_attn() {
        let mut set = minimal();
        set.flash_attn = true;
        let args = set.to_args();
        let args: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();
        assert!(args.contains(&"--diffusion-fa"));
        assert!(!args.contains(&"--flash-attn"));
    }

    #[test]
    fn boolean_flags_are_omitted_when_false() {
        let args = minimal().to_args();
        let args: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();
        assert!(!args.contains(&"--vae-tiling"));
        assert!(!args.contains(&"--diffusion-fa"));
    }

    #[test]
    fn fingerprint_is_stable_for_identical_sets() {
        assert_eq!(minimal().fingerprint(), minimal().fingerprint());
    }

    #[test]
    fn fingerprint_changes_with_diffusion_model() {
        let mut other = minimal();
        other.diffusion_model = Some(PathBuf::from("/models/other.gguf"));
        assert_ne!(minimal().fingerprint(), other.fingerprint());
    }

    #[test]
    fn fingerprint_changes_with_vae() {
        let mut other = minimal();
        other.vae = None;
        assert_ne!(minimal().fingerprint(), other.fingerprint());
    }

    #[test]
    fn fingerprint_changes_with_text_encoder() {
        let mut other = minimal();
        other.text_encoder = Some(PathBuf::from("/models/other-encoder.gguf"));
        assert_ne!(minimal().fingerprint(), other.fingerprint());
    }

    #[test]
    fn fingerprint_changes_with_weight_type() {
        let mut other = minimal();
        other.weight_type = Some(WeightTypeArg::Q4K);
        assert_ne!(minimal().fingerprint(), other.fingerprint());
    }

    #[test]
    fn fingerprint_changes_with_threads() {
        let mut other = minimal();
        other.threads = Some(8);
        assert_ne!(minimal().fingerprint(), other.fingerprint());
    }

    #[test]
    fn fingerprint_changes_with_backend() {
        let mut other = minimal();
        other.backend = Some(Backend::Vulkan);
        assert_ne!(minimal().fingerprint(), other.fingerprint());
    }

    #[test]
    fn fingerprint_changes_with_vae_tiling() {
        let mut other = minimal();
        other.vae_tiling = true;
        assert_ne!(minimal().fingerprint(), other.fingerprint());
    }

    #[test]
    fn fingerprint_changes_with_flash_attn() {
        let mut other = minimal();
        other.flash_attn = true;
        assert_ne!(minimal().fingerprint(), other.fingerprint());
    }

    #[test]
    fn fingerprint_changes_with_release_tag() {
        let mut other = minimal();
        other.release_tag = "master-891-deadbee".to_owned();
        assert_ne!(minimal().fingerprint(), other.fingerprint());
    }

    #[test]
    fn fingerprint_is_a_64_char_hex_string() {
        let fp = minimal().fingerprint();
        assert_eq!(fp.len(), 64);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
