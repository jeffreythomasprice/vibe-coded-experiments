use crate::cli::GenerateArgs;

pub const DEFAULT_JITTER: f32 = 0.12;

// Mirrors sd-server's own defaults (`--steps`/`--cfg-scale`/`--guidance` in
// `sd-server --help`), materialized here so an unset param still has something
// to jitter around instead of silently staying fixed.
const SERVER_DEFAULT_STEPS: i32 = 20;
const SERVER_DEFAULT_CFG_SCALE: f32 = 7.0;
const SERVER_DEFAULT_GUIDANCE: f32 = 3.5;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UsedParams {
    pub steps: Option<i32>,
    pub cfg_scale: Option<f32>,
    pub guidance: Option<f32>,
}

/// The params to actually send for copy `index` of a run seeded at `seed`.
/// Copy 0 and `amount == 0.0` both pass `args`'s values through unchanged
/// (including `None`), so the first copy and a disabled jitter are byte-for-byte
/// what this crate sent before jitter existed. Otherwise, any of steps/cfg_scale
/// /guidance left unset is filled from `SERVER_DEFAULT_*` before jittering, so a
/// preset that sets none of the three (e.g. `flux`) still gets real variance.
pub fn params_for(args: &GenerateArgs, seed: i64, index: u32, amount: f32) -> UsedParams {
    if index == 0 || amount == 0.0 {
        return UsedParams {
            steps: args.steps,
            cfg_scale: args.cfg_scale,
            guidance: args.guidance,
        };
    }

    let mut state = (seed as u64) ^ u64::from(index).wrapping_mul(0x9E37_79B9_7F4A_7C15);

    // Fixed draw order: steps, cfg_scale, guidance. Add any future jittered
    // param after these three so existing draws don't shift.
    let steps_base = args.steps.unwrap_or(SERVER_DEFAULT_STEPS);
    let steps = ((steps_base as f32 * multiplier(&mut state, amount)).round().max(1.0)) as i32;

    let cfg_base = args.cfg_scale.unwrap_or(SERVER_DEFAULT_CFG_SCALE);
    let cfg_scale = (cfg_base * multiplier(&mut state, amount)).max(1.0);

    let guidance_base = args.guidance.unwrap_or(SERVER_DEFAULT_GUIDANCE);
    let guidance = (guidance_base * multiplier(&mut state, amount)).max(0.0);

    UsedParams {
        steps: Some(steps),
        cfg_scale: Some(cfg_scale),
        guidance: Some(guidance),
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A pseudo-random value in `[0, 1)`.
fn unit(state: &mut u64) -> f32 {
    (splitmix64(state) >> 40) as f32 / (1u64 << 24) as f32
}

/// A pseudo-random value in `[1 - amount, 1 + amount)`.
fn multiplier(state: &mut u64, amount: f32) -> f32 {
    1.0 + amount * (2.0 * unit(state) - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

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
    fn copy_zero_is_never_jittered() {
        let args = parse(&["a prompt", "--steps", "20", "--cfg-scale", "7.0", "--guidance", "3.5"]);
        let used = params_for(&args, 42, 0, 0.12);
        assert_eq!(used.steps, Some(20));
        assert_eq!(used.cfg_scale, Some(7.0));
        assert_eq!(used.guidance, Some(3.5));

        let unset = parse(&["a prompt"]);
        let used = params_for(&unset, 42, 0, 0.12);
        assert_eq!(used, UsedParams::default());
    }

    #[test]
    fn same_seed_and_index_reproduce_identical_params() {
        let args = parse(&["a prompt"]);
        let first = params_for(&args, 42, 1, 0.12);
        let second = params_for(&args, 42, 1, 0.12);
        assert_eq!(first, second);

        let third = params_for(&args, 42, 2, 0.12);
        assert_ne!(first, third);
    }

    #[test]
    fn jitter_zero_passes_params_through_unchanged() {
        let args = parse(&["a prompt"]);
        let used = params_for(&args, 42, 3, 0.0);
        assert_eq!(used, UsedParams::default());
    }

    #[test]
    fn unset_params_are_materialized_from_server_defaults() {
        let args = parse(&["a prompt"]);
        let used = params_for(&args, 42, 1, 0.12);
        let steps = used.steps.unwrap();
        let cfg_scale = used.cfg_scale.unwrap();
        let guidance = used.guidance.unwrap();
        assert!((18..=22).contains(&steps), "steps {steps} out of range");
        assert!((6.16..=7.84).contains(&cfg_scale), "cfg_scale {cfg_scale} out of range");
        assert!((3.08..=3.92).contains(&guidance), "guidance {guidance} out of range");
    }

    #[test]
    fn turbo_values_stay_within_floors() {
        let args = parse(&["a prompt", "--steps", "4", "--cfg-scale", "1.0", "--guidance", "0.0"]);
        for index in 1..64 {
            let used = params_for(&args, 7, index, 0.12);
            assert!(used.steps.unwrap() >= 1);
            assert!(used.cfg_scale.unwrap() >= 1.0);
            assert_eq!(used.guidance.unwrap(), 0.0);
        }
    }

    #[test]
    fn multiplier_stays_within_the_requested_band() {
        let amount = 0.2;
        let mut state = 12345u64;
        let mut sum = 0.0;
        let draws = 1000;
        for _ in 0..draws {
            let m = multiplier(&mut state, amount);
            assert!((1.0 - amount..1.0 + amount).contains(&m), "multiplier {m} out of band");
            sum += m;
        }
        let mean = sum / draws as f32;
        assert!((mean - 1.0).abs() < 0.02, "mean {mean} drifted too far from 1.0");
    }
}
