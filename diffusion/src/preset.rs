use std::collections::BTreeMap;
use std::fmt::Display;
use std::str::FromStr;

use clap::ValueEnum;
use serde::{Deserialize, Deserializer};

use crate::cli::{Backend, GenerateArgs, Sampler, WeightTypeArg};
use crate::error::AppError;
use crate::models::ModelRef;

/// A named bundle of `generate` arguments, applied with `--preset <name>`.
///
/// Every field mirrors a `generate` flag, except the per-invocation ones
/// (`prompt`, `--output`, `--copies`, `--show`, `--json`, `--seed`) which only make
/// sense on the command line.
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Preset {
    #[serde(deserialize_with = "from_str_opt")]
    pub model: Option<ModelRef>,
    #[serde(deserialize_with = "from_str_opt")]
    pub diffusion_model: Option<ModelRef>,
    #[serde(deserialize_with = "from_str_opt")]
    pub vae: Option<ModelRef>,
    #[serde(deserialize_with = "from_str_opt")]
    pub clip_l: Option<ModelRef>,
    #[serde(deserialize_with = "from_str_opt")]
    pub clip_g: Option<ModelRef>,
    #[serde(deserialize_with = "from_str_opt")]
    pub t5xxl: Option<ModelRef>,
    #[serde(deserialize_with = "from_str_opt")]
    pub taesd: Option<ModelRef>,
    #[serde(deserialize_with = "value_enum_opt")]
    pub weight_type: Option<WeightTypeArg>,
    #[serde(deserialize_with = "value_enum_opt")]
    pub backend: Option<Backend>,
    #[serde(deserialize_with = "value_enum_opt")]
    pub sampler: Option<Sampler>,
    #[serde(deserialize_with = "clip_skip_opt")]
    pub clip_skip: Option<u8>,
    pub vae_tiling: Option<bool>,
    pub flash_attn: Option<bool>,
    pub threads: Option<i32>,
    pub negative: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub steps: Option<i32>,
    pub cfg_scale: Option<f32>,
    pub guidance: Option<f32>,
}

fn from_str_opt<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: Display,
{
    let value: Option<String> = Option::deserialize(deserializer)?;
    value
        .map(|s| s.parse().map_err(serde::de::Error::custom))
        .transpose()
}

fn value_enum_opt<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: ValueEnum,
{
    let value: Option<String> = Option::deserialize(deserializer)?;
    value
        .map(|s| {
            T::from_str(&s, false).map_err(|_| {
                let valid = T::value_variants()
                    .iter()
                    .filter_map(ValueEnum::to_possible_value)
                    .map(|pv| pv.get_name().to_owned())
                    .collect::<Vec<_>>()
                    .join(", ");
                serde::de::Error::custom(format!("invalid value {s:?}, expected one of: {valid}"))
            })
        })
        .transpose()
}

fn clip_skip_opt<'de, D>(deserializer: D) -> Result<Option<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<u8> = Option::deserialize(deserializer)?;
    match value {
        Some(v) if v > 2 => Err(serde::de::Error::custom(format!(
            "clip_skip must be 0, 1, or 2, got {v}"
        ))),
        other => Ok(other),
    }
}

/// Fill any unset field of `args` from `args.preset`, logging a warning for
/// every field the command line already set. A no-op when `--preset` wasn't
/// passed.
pub fn apply(args: &mut GenerateArgs, presets: &BTreeMap<String, Preset>) -> Result<(), AppError> {
    let Some(name) = args.preset.clone() else {
        return Ok(());
    };
    let Some(preset) = presets.get(&name) else {
        let available = if presets.is_empty() {
            "none".to_owned()
        } else {
            presets.keys().cloned().collect::<Vec<_>>().join(", ")
        };
        return Err(AppError::UnknownPreset { name, available });
    };

    macro_rules! merge {
        ($($field:ident => $flag:literal),* $(,)?) => {$(
            if let Some(value) = &preset.$field {
                match &args.$field {
                    None => args.$field = Some(value.clone()),
                    Some(cli) => tracing::warn!(
                        preset = %name,
                        arg = $flag,
                        preset_value = ?value,
                        cli_value = ?cli,
                        "command-line argument overrides preset"
                    ),
                }
            }
        )*};
    }

    merge!(
        model => "--model",
        diffusion_model => "--diffusion-model",
        vae => "--vae",
        clip_l => "--clip-l",
        clip_g => "--clip-g",
        t5xxl => "--t5xxl",
        taesd => "--taesd",
        weight_type => "--weight-type",
        backend => "--backend",
        sampler => "--sampler",
        clip_skip => "--clip-skip",
        vae_tiling => "--vae-tiling",
        flash_attn => "--flash-attn",
        threads => "--threads",
        negative => "--negative",
        width => "--width",
        height => "--height",
        steps => "--steps",
        cfg_scale => "--cfg-scale",
        guidance => "--guidance",
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Command};
    use clap::Parser;

    fn parse(args: &[&str]) -> GenerateArgs {
        let mut full = vec!["diffusion", "generate"];
        full.extend_from_slice(args);
        match Cli::try_parse_from(full).unwrap().command {
            Command::Generate(args) => *args,
            other => panic!("expected Command::Generate, got {other:?}"),
        }
    }

    #[test]
    fn preset_fills_unset_args() {
        let mut args = parse(&["a prompt", "--preset", "turbo"]);
        let mut presets = BTreeMap::new();
        presets.insert(
            "turbo".to_owned(),
            Preset {
                model: Some("stabilityai/sd-turbo".parse().unwrap()),
                steps: Some(4),
                cfg_scale: Some(1.0),
                guidance: Some(0.0),
                ..Default::default()
            },
        );

        apply(&mut args, &presets).unwrap();

        assert_eq!(args.model, Some("stabilityai/sd-turbo".parse().unwrap()));
        assert_eq!(args.steps, Some(4));
        assert_eq!(args.cfg_scale, Some(1.0));
        assert_eq!(args.guidance, Some(0.0));
    }

    #[test]
    fn explicit_flag_beats_preset() {
        let mut args = parse(&["a prompt", "--preset", "turbo", "--steps", "8"]);
        let mut presets = BTreeMap::new();
        presets.insert(
            "turbo".to_owned(),
            Preset {
                steps: Some(4),
                ..Default::default()
            },
        );

        apply(&mut args, &presets).unwrap();

        assert_eq!(args.steps, Some(8));
    }

    #[test]
    fn explicit_false_flag_beats_preset_true() {
        let mut args = parse(&["a prompt", "--preset", "aggressive", "--flash-attn=false"]);
        let mut presets = BTreeMap::new();
        presets.insert(
            "aggressive".to_owned(),
            Preset {
                flash_attn: Some(true),
                ..Default::default()
            },
        );

        apply(&mut args, &presets).unwrap();

        assert_eq!(args.flash_attn, Some(false));
    }

    #[test]
    fn unknown_preset_lists_available() {
        let mut args = parse(&["a prompt", "--preset", "nope"]);
        let mut presets = BTreeMap::new();
        presets.insert("flux".to_owned(), Preset::default());
        presets.insert("turbo".to_owned(), Preset::default());

        let err = apply(&mut args, &presets).unwrap_err();

        match err {
            AppError::UnknownPreset { name, available } => {
                assert_eq!(name, "nope");
                assert_eq!(available, "flux, turbo");
            }
            other => panic!("expected UnknownPreset, got {other:?}"),
        }
    }

    #[test]
    fn no_preset_flag_is_a_no_op() {
        let mut args = parse(&["a prompt"]);
        let presets = BTreeMap::new();

        apply(&mut args, &presets).unwrap();

        assert_eq!(args.model, None);
    }
}
