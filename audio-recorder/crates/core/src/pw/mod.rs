mod thread;

use std::{
    sync::mpsc,
    thread::JoinHandle,
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender};

use crate::device::{DeviceInfo, DeviceKey};

#[derive(Debug, thiserror::Error)]
pub enum PwError {
    #[error("failed to connect to PipeWire: {0}")]
    Connect(String),
    #[error("PipeWire thread exited unexpectedly")]
    ThreadGone,
    #[error("timed out waiting for PipeWire to enumerate devices")]
    SyncTimeout,
}

#[derive(Debug)]
pub enum CaptureMsg {
    Format { rate: u32, channels: u32 },
    Samples { arrived: Instant, data: Vec<f32> },
}

#[derive(Debug, Clone)]
pub enum EndReason {
    DeviceRemoved,
    Error(String),
}

#[derive(Debug)]
pub enum PwEvent {
    DeviceAdded(DeviceInfo),
    DeviceRemoved(DeviceKey),
    CaptureEnded { id: u64, reason: EndReason },
    Synced,
    Stopped,
}

pub(crate) enum PwCommand {
    StartCapture { id: u64, key: DeviceKey, tx: Sender<CaptureMsg> },
    StopCapture { id: u64 },
    Shutdown,
}

pub struct PwHandle {
    cmd: pipewire::channel::Sender<PwCommand>,
    pub events: Receiver<PwEvent>,
    join: Option<JoinHandle<()>>,
}

impl PwHandle {
    pub fn spawn() -> Result<Self, PwError> {
        let (event_tx, events) = crossbeam_channel::unbounded();
        let (cmd, cmd_rx) = pipewire::channel::channel();
        let (init_tx, init_rx) = mpsc::channel();
        let join = std::thread::Builder::new()
            .name("pipewire".into())
            .spawn(move || thread::run(cmd_rx, event_tx, init_tx))
            .map_err(|e| PwError::Connect(e.to_string()))?;
        match init_rx.recv() {
            Ok(Ok(())) => Ok(Self { cmd, events, join: Some(join) }),
            Ok(Err(e)) => {
                let _ = join.join();
                Err(PwError::Connect(e))
            }
            Err(_) => Err(PwError::ThreadGone),
        }
    }

    pub fn wait_synced(&self, timeout: Duration) -> Result<Vec<PwEvent>, PwError> {
        let deadline = Instant::now() + timeout;
        let mut seen = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(PwEvent::Synced) => return Ok(seen),
                Ok(ev) => seen.push(ev),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => return Err(PwError::SyncTimeout),
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return Err(PwError::ThreadGone),
            }
        }
    }

    pub fn start_capture(&self, id: u64, key: DeviceKey, tx: Sender<CaptureMsg>) -> Result<(), PwError> {
        self.cmd.send(PwCommand::StartCapture { id, key, tx }).map_err(|_| PwError::ThreadGone)
    }

    pub fn stop_capture(&self, id: u64) -> Result<(), PwError> {
        self.cmd.send(PwCommand::StopCapture { id }).map_err(|_| PwError::ThreadGone)
    }
}

impl Drop for PwHandle {
    fn drop(&mut self) {
        let _ = self.cmd.send(PwCommand::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
