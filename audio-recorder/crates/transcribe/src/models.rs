use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::PathBuf,
};

use audio_recorder_core::config::{ModelPath, TranscriptionSettings};

const WHISPER_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin";
const LIVE_WHISPER_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin";
const VAD_URL: &str = "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v5.1.2.bin";
const SEGMENTATION_URL: &str = "https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0/resolve/main/model.onnx";
const EMBEDDING_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/nemo_en_titanet_small.onnx";

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("{label} model not found at {} (set explicitly in config, so it is not downloaded)", path.display())]
    Missing { label: &'static str, path: PathBuf },
    #[error("failed to download {url}: {source}")]
    Download { url: &'static str, source: Box<ureq::Error> },
    #[error("model file error at {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("cancelled")]
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct ModelFile {
    pub label: &'static str,
    pub path: PathBuf,
    pub explicit: bool,
    pub url: &'static str,
}

impl ModelFile {
    pub fn present(&self) -> bool {
        self.path.is_file()
    }
}

pub fn all(settings: &TranscriptionSettings) -> Vec<ModelFile> {
    let file = |label, m: &ModelPath, url| ModelFile { label, path: m.path.clone(), explicit: m.explicit, url };
    let mut files = vec![
        file("whisper", &settings.whisper_model, WHISPER_URL),
        file("vad", &settings.vad_model, VAD_URL),
        file("segmentation", &settings.segmentation_model, SEGMENTATION_URL),
        file("embedding", &settings.embedding_model, EMBEDDING_URL),
    ];
    if settings.live.enabled {
        files.insert(1, file("live whisper", &settings.live.whisper_model, LIVE_WHISPER_URL));
    }
    files
}

pub fn live_required(settings: &TranscriptionSettings) -> Vec<ModelFile> {
    all(settings).into_iter().filter(|m| matches!(m.label, "live whisper" | "vad")).collect()
}

pub fn required(settings: &TranscriptionSettings) -> Vec<ModelFile> {
    all(settings)
        .into_iter()
        .filter(|m| m.label != "live whisper")
        .filter(|m| settings.diarize || !matches!(m.label, "segmentation" | "embedding"))
        .collect()
}

pub fn ensure(
    files: &[ModelFile],
    progress: &mut dyn FnMut(&'static str, u64, Option<u64>),
    cancelled: &dyn Fn() -> bool,
) -> Result<(), ModelError> {
    for file in files.iter().filter(|f| !f.present()) {
        if file.explicit {
            return Err(ModelError::Missing { label: file.label, path: file.path.clone() });
        }
        download(file, progress, cancelled)?;
    }
    Ok(())
}

fn download(
    file: &ModelFile,
    progress: &mut dyn FnMut(&'static str, u64, Option<u64>),
    cancelled: &dyn Fn() -> bool,
) -> Result<(), ModelError> {
    let io_err = |source| ModelError::Io { path: file.path.clone(), source };
    if let Some(parent) = file.path.parent() {
        fs::create_dir_all(parent).map_err(io_err)?;
    }
    tracing::info!("downloading {} model from {}", file.label, file.url);
    let response = ureq::get(file.url)
        .call()
        .map_err(|source| ModelError::Download { url: file.url, source: Box::new(source) })?;
    let total = response.body().content_length();
    let mut reader = response.into_body().into_reader();
    let mut part = file.path.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let mut out = File::create(&part).map_err(|source| ModelError::Io { path: part.clone(), source })?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        if cancelled() {
            drop(out);
            let _ = fs::remove_file(&part);
            return Err(ModelError::Cancelled);
        }
        let n = reader.read(&mut buf).map_err(|source| ModelError::Io { path: part.clone(), source })?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|source| ModelError::Io { path: part.clone(), source })?;
        done += n as u64;
        progress(file.label, done, total);
    }
    out.flush().map_err(|source| ModelError::Io { path: part.clone(), source })?;
    drop(out);
    fs::rename(&part, &file.path).map_err(io_err)
}
