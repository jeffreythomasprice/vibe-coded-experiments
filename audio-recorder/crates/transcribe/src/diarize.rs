use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use sherpa_rs::diarize::{Diarize, DiarizeConfig};

use crate::assemble::Turn;

#[derive(Debug, thiserror::Error)]
pub enum DiarizeFailure {
    #[error("failed to initialize diarization: {0}")]
    Init(String),
    #[error("cancelled")]
    Cancelled,
}

pub struct Diarizer {
    inner: Diarize,
}

impl Diarizer {
    pub fn load(segmentation: &Path, embedding: &Path, num_speakers: Option<i32>, threshold: Option<f32>) -> Result<Self, DiarizeFailure> {
        let config = DiarizeConfig {
            num_clusters: Some(num_speakers.unwrap_or(-1)),
            threshold,
            min_duration_on: Some(0.2),
            min_duration_off: Some(0.5),
            provider: Some("cpu".to_string()),
            debug: false,
        };
        let inner = Diarize::new(segmentation, embedding, config).map_err(|e| DiarizeFailure::Init(e.to_string()))?;
        Ok(Self { inner })
    }

    pub fn turns(
        &mut self,
        samples: &[f32],
        progress: Arc<dyn Fn(f32) + Send + Sync>,
        cancel: Arc<AtomicBool>,
    ) -> Result<Vec<Turn>, DiarizeFailure> {
        let flag = cancel.clone();
        let callback = Box::new(move |done: i32, total: i32| {
            if total > 0 {
                progress(done as f32 / total as f32);
            }
            i32::from(flag.load(Ordering::Relaxed))
        });
        let result = self.inner.compute(samples.to_vec(), Some(callback));
        if cancel.load(Ordering::Relaxed) {
            return Err(DiarizeFailure::Cancelled);
        }
        match result {
            Ok(segments) => Ok(segments
                .into_iter()
                .map(|s| Turn { start_s: s.start as f64, end_s: s.end as f64, speaker: s.speaker })
                .collect()),
            Err(e) => {
                tracing::warn!("diarization produced no speaker turns: {e}");
                Ok(Vec::new())
            }
        }
    }
}
