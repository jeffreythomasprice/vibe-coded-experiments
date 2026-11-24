use std::{
    io,
    path::{Path, PathBuf},
};

use audio_recorder_schema::{
    SchemaError,
    analysis::{Analysis, DeviceAnalysis},
    from_json_str,
    manifest::Manifest,
    mix::{ChannelGain, MixSettings},
    to_json_string_pretty,
};
use ebur128::{EbuR128, Mode};

use crate::{
    atomic,
    mix::{self, MixError},
    mixdown::{self, MixdownError, OUT_CHANNELS, OUT_RATE},
    session::manifest::{self, ManifestError},
};

pub const ANALYSIS_FILE: &str = "analysis.json";
pub const TARGET_LUFS: f64 = -20.0;
pub const MIN_GAIN: f64 = 0.25;
pub const MAX_GAIN: f64 = 2.0;

#[derive(Debug, thiserror::Error)]
pub enum BalanceError {
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Mix(#[from] MixError),
    #[error(transparent)]
    Mixdown(#[from] MixdownError),
    #[error("loudness analysis failed: {0}")]
    Loudness(#[from] ebur128::Error),
    #[error("failed to write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },
    #[error("could not serialize analysis: {0}")]
    Serialize(#[from] SchemaError),
}

fn load_cache(dir: &Path) -> Analysis {
    let path = dir.join(ANALYSIS_FILE);
    let empty = Analysis { version: 1, devices: Default::default() };
    match std::fs::read_to_string(&path) {
        Ok(text) => from_json_str(&text).unwrap_or_else(|e| {
            tracing::warn!("ignoring unreadable {}: {e}", path.display());
            empty
        }),
        Err(_) => empty,
    }
}

fn device_files(manifest: &Manifest, dir: &Path, device: &str) -> Vec<String> {
    manifest
        .segments
        .iter()
        .filter(|s| s.device == device && dir.join(&s.file).is_file())
        .map(|s| s.file.clone())
        .collect()
}

fn measure(dir: &Path, files: &[String]) -> Result<DeviceAnalysis, BalanceError> {
    let mut meter = EbuR128::new(OUT_CHANNELS as u32, OUT_RATE, Mode::I | Mode::SAMPLE_PEAK)?;
    let mut failure = None;
    for file in files {
        let result = mixdown::read_resampled(&dir.join(file), &mut |block| {
            if failure.is_none() {
                if let Err(e) = meter.add_frames_f32(block) {
                    failure = Some(e);
                }
            }
        });
        result?;
    }
    if let Some(e) = failure {
        return Err(e.into());
    }
    let loudness = meter.loudness_global()?;
    let peak = (0..OUT_CHANNELS as u32).map(|c| meter.sample_peak(c)).collect::<Result<Vec<_>, _>>()?;
    Ok(DeviceAnalysis {
        files: files.to_vec(),
        loudness_lufs: loudness.is_finite().then_some(loudness),
        peak: peak.into_iter().fold(0.0, f64::max),
    })
}

pub fn analyze(dir: &Path) -> Result<Analysis, BalanceError> {
    let manifest = manifest::load_recovering(dir)?;
    let cacheable = manifest.ended_at.is_some() && !manifest::is_recording(dir);
    let cached = load_cache(dir);
    let mut result = Analysis { version: 1, devices: Default::default() };
    let mut dirty = false;
    let mut devices: Vec<&String> = manifest.devices.keys().collect();
    devices.sort();
    for device in devices {
        let files = device_files(&manifest, dir, device);
        if files.is_empty() {
            continue;
        }
        let entry = match cached.devices.get(device) {
            Some(c) if cacheable && c.files == files => c.clone(),
            _ => {
                dirty = true;
                measure(dir, &files)?
            }
        };
        result.devices.insert(device.clone(), entry);
    }
    dirty |= result.devices.len() != cached.devices.len();
    if cacheable && dirty {
        let path = dir.join(ANALYSIS_FILE);
        let text = to_json_string_pretty(&result)?;
        atomic::write(&path, text.as_bytes()).map_err(|source| BalanceError::Write { path, source })?;
    }
    Ok(result)
}

pub fn rename_files(dir: &Path, map: &std::collections::HashMap<String, String>) -> Result<(), BalanceError> {
    let path = dir.join(ANALYSIS_FILE);
    if !path.is_file() {
        return Ok(());
    }
    let mut analysis = load_cache(dir);
    let mut changed = false;
    for entry in analysis.devices.values_mut() {
        for file in &mut entry.files {
            if let Some(new) = map.get(file) {
                *file = new.clone();
                changed = true;
            }
        }
    }
    if changed {
        let text = to_json_string_pretty(&analysis)?;
        atomic::write(&path, text.as_bytes()).map_err(|source| BalanceError::Write { path, source })?;
    }
    Ok(())
}

pub fn auto_mix(dir: &Path) -> Result<MixSettings, BalanceError> {
    let analysis = analyze(dir)?;
    let mut settings = mix::default_settings();
    settings.leveling = true;
    settings.auto = true;
    for (device, a) in &analysis.devices {
        let Some(lufs) = a.loudness_lufs else { continue };
        let gain = 10f64.powf((TARGET_LUFS - lufs) / 20.0).clamp(MIN_GAIN, MAX_GAIN);
        settings.devices.insert(device.clone(), ChannelGain { gain, muted: false });
    }
    Ok(settings)
}

pub fn ensure_default(dir: &Path) -> Result<Option<MixSettings>, BalanceError> {
    let mix_path = dir.join(mix::MIX_FILE);
    if mix_path.exists() || manifest::is_recording(dir) {
        return Ok(None);
    }
    let manifest = manifest::load_recovering(dir)?;
    if manifest.ended_at.is_none() || manifest.segments.is_empty() {
        return Ok(None);
    }
    let settings = auto_mix(dir)?;
    if mix_path.exists() {
        return Ok(None);
    }
    mix::save(dir, &settings)?;
    Ok(Some(settings))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use audio_recorder_schema::manifest::{Direction, ManifestDevice, Segment, SegmentStatus};

    use super::*;

    fn write_sine(path: &Path, amp: f32, seconds: u32) {
        let spec = hound::WavSpec { channels: 1, sample_rate: 48000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..48000 * seconds {
            w.write_sample(amp * (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / 48000.0).sin()).unwrap();
        }
        w.finalize().unwrap();
    }

    fn session(name: &str, devices: &[(&str, f32)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("audio-recorder-balance-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("tracks")).unwrap();
        let mut m = manifest::new_manifest("2026-01-01T00:00:00Z".into());
        m.ended_at = Some("2026-01-01T00:00:10Z".into());
        for (i, (device, amp)) in devices.iter().enumerate() {
            let file = format!("tracks/{i}.wav");
            write_sine(&dir.join(&file), *amp, 4);
            m.devices.insert(device.to_string(), ManifestDevice { description: device.to_string(), direction: Direction::Input });
            m.segments.push(Segment {
                channels: NonZeroU64::new(1).unwrap(),
                device: device.to_string(),
                end_offset_ns: Some(4_000_000_000),
                file,
                frames: 48000 * 4,
                sample_rate: NonZeroU64::new(48000).unwrap(),
                start_offset_ns: 0,
                status: SegmentStatus::Complete,
            });
        }
        manifest::save(&dir, &m).unwrap();
        dir
    }

    #[test]
    fn balances_quiet_and_loud_devices_to_equal_loudness() {
        let dir = session("eq", &[("input:quiet", 0.1), ("input:loud", 0.4)]);
        let m = auto_mix(&dir).unwrap();
        assert!(m.auto && m.leveling);
        let q = m.devices["input:quiet"].gain * 0.1;
        let l = m.devices["input:loud"].gain * 0.4;
        let db = 20.0 * (q / l).log10();
        assert!(db.abs() < 0.5, "{db}");
    }

    #[test]
    fn silent_device_keeps_unity_and_cache_is_reused() {
        let dir = session("silent", &[("input:a", 0.0), ("input:b", 0.3)]);
        let m = auto_mix(&dir).unwrap();
        assert!(!m.devices.contains_key("input:a"));
        let first = std::fs::metadata(dir.join(ANALYSIS_FILE)).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        analyze(&dir).unwrap();
        assert_eq!(std::fs::metadata(dir.join(ANALYSIS_FILE)).unwrap().modified().unwrap(), first);
    }

    #[test]
    fn ensure_default_writes_once_and_never_overwrites() {
        let dir = session("ensure", &[("input:a", 0.2)]);
        assert!(ensure_default(&dir).unwrap().is_some());
        let mut manual = mix::default_settings();
        manual.devices.insert("input:a".into(), ChannelGain { gain: 1.0, muted: true });
        mix::save(&dir, &manual).unwrap();
        assert!(ensure_default(&dir).unwrap().is_none());
        assert!(mix::load(&dir).unwrap().devices["input:a"].muted);
    }
}
