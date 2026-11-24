use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperError, WhisperState, WhisperVadContext,
    WhisperVadContextParams, WhisperVadParams,
};

use crate::assemble::Word;

#[derive(Debug, thiserror::Error)]
pub enum WhisperFailure {
    #[error("whisper model path is not valid UTF-8: {0}")]
    BadPath(String),
    #[error("failed to load whisper model {path}: {source}")]
    Load { path: String, source: WhisperError },
    #[error("whisper failed: {0}")]
    Run(WhisperError),
    #[error("cancelled")]
    Cancelled,
}

// whisper-rs 0.16's set_abort_callback_safe pairs a double-boxed closure with a trampoline for the
// unboxed type, so the callback reads garbage and aborts every run; use the raw API instead.
unsafe extern "C" fn abort_flag(data: *mut std::ffi::c_void) -> bool {
    unsafe { &*(data as *const AtomicBool) }.load(Ordering::Relaxed)
}

const RATE: f64 = 16_000.0;
const MERGE_GAP_S: f64 = 1.0;
const PAD_S: f64 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoding {
    Beam,
    Greedy,
}

pub struct Whisper {
    ctx: WhisperContext,
    vad: WhisperVadContext,
    language: String,
}

pub(crate) fn speech_regions(vad: &mut WhisperVadContext, samples: &[f32]) -> Result<Vec<(usize, usize)>, WhisperFailure> {
    let mut params = WhisperVadParams::new();
    params.set_min_speech_duration(250);
    params.set_min_silence_duration(300);
    params.set_speech_pad(30);
    let segments = vad.segments_from_samples(params, samples).map_err(WhisperFailure::Run)?;
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for seg in segments {
        let (start, end) = ((seg.start as f64 / 100.0 - PAD_S).max(0.0), seg.end as f64 / 100.0 + PAD_S);
        match merged.last_mut() {
            Some(last) if start - last.1 < MERGE_GAP_S => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    Ok(merged
        .into_iter()
        .map(|(a, b)| ((a * RATE) as usize, ((b * RATE) as usize).min(samples.len())))
        .filter(|(a, b)| b > a)
        .collect())
}

impl Whisper {
    pub fn load(model: &Path, vad_model: &Path, language: &str, gpu: bool) -> Result<Self, WhisperFailure> {
        let path = model.to_str().ok_or_else(|| WhisperFailure::BadPath(model.display().to_string()))?;
        let vad = vad_model.to_str().ok_or_else(|| WhisperFailure::BadPath(vad_model.display().to_string()))?;
        let mut params = WhisperContextParameters::default();
        params.use_gpu(gpu);
        let ctx = WhisperContext::new_with_params(path, params)
            .map_err(|source| WhisperFailure::Load { path: path.to_string(), source })?;
        let mut vad_params = WhisperVadContextParams::new();
        vad_params.set_use_gpu(false);
        let vad = WhisperVadContext::new(vad, vad_params)
            .map_err(|source| WhisperFailure::Load { path: vad.to_string(), source })?;
        Ok(Self { ctx, vad, language: language.to_string() })
    }

    pub fn new_state(&self) -> Result<WhisperState, WhisperFailure> {
        self.ctx.create_state().map_err(WhisperFailure::Run)
    }

    pub fn regions(&mut self, samples: &[f32]) -> Result<Vec<(usize, usize)>, WhisperFailure> {
        speech_regions(&mut self.vad, samples)
    }

    /// Words with times relative to the start of `samples`.
    pub fn decode(
        &self,
        state: &mut WhisperState,
        samples: &[f32],
        decoding: Decoding,
        progress: Option<Arc<dyn Fn(i32) + Send + Sync>>,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Vec<Word>, WhisperFailure> {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8)) as i32;
        let strategy = match decoding {
            Decoding::Beam => SamplingStrategy::BeamSearch { beam_size: 5, patience: -1.0 },
            Decoding::Greedy => SamplingStrategy::Greedy { best_of: 1 },
        };
        let mut params = FullParams::new(strategy);
        params.set_n_threads(threads);
        params.set_language(Some(&self.language));
        params.set_token_timestamps(true);
        params.set_split_on_word(true);
        params.set_max_len(1);
        params.set_suppress_nst(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        if decoding == Decoding::Greedy {
            params.set_no_context(true);
        }
        if let Some(report) = progress {
            params.set_progress_callback_safe(move |p: i32| report(p));
        }
        // SAFETY: `cancel` outlives `state.full` below, the only user of this pointer.
        unsafe {
            params.set_abort_callback(Some(abort_flag));
            params.set_abort_callback_user_data(Arc::as_ptr(cancel) as *mut std::ffi::c_void);
        }

        let result = state.full(params, samples);
        if cancel.load(Ordering::Relaxed) {
            return Err(WhisperFailure::Cancelled);
        }
        result.map_err(WhisperFailure::Run)?;
        Ok(state
            .as_iter()
            .filter_map(|seg| {
                let text = seg.to_str_lossy().ok()?.into_owned();
                Some(Word {
                    start_s: seg.start_timestamp() as f64 / 100.0,
                    end_s: seg.end_timestamp() as f64 / 100.0,
                    text,
                })
            })
            .collect())
    }

    pub fn transcribe(
        &mut self,
        samples: &[f32],
        progress: Arc<dyn Fn(f32) + Send + Sync>,
        cancel: Arc<AtomicBool>,
    ) -> Result<Vec<Word>, WhisperFailure> {
        let regions = self.regions(samples)?;
        let total: usize = regions.iter().map(|(a, b)| b - a).sum::<usize>().max(1);
        let mut state = self.new_state()?;
        let mut words = Vec::new();
        let mut done = 0usize;

        for (start, end) in regions {
            if cancel.load(Ordering::Relaxed) {
                return Err(WhisperFailure::Cancelled);
            }
            let (base, len) = (done, end - start);
            let report = progress.clone();
            let on_progress: Arc<dyn Fn(i32) + Send + Sync> =
                Arc::new(move |p| report((base as f32 + len as f32 * p as f32 / 100.0) / total as f32));
            let offset = start as f64 / RATE;
            let region = self.decode(&mut state, &samples[start..end], Decoding::Beam, Some(on_progress), &cancel)?;
            words.extend(region.into_iter().map(|w| Word { start_s: w.start_s + offset, end_s: w.end_s + offset, text: w.text }));
            done += len;
        }
        Ok(words)
    }
}
