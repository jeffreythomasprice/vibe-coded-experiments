use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use level_cmd::{CommandError, ErrorResponse, Request, Response};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::config::Config;
use crate::wire::{
    ChangedEvent, ClientMsg, ControlAction, HelloResult, PROTOCOL, ServerMsg, SnapshotEvent,
};

const SPAWN_WAIT: Duration = Duration::from_secs(3);
const REPLY_WAIT: Duration = Duration::from_secs(120);

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("cannot connect to {path}: {source}")]
    Connect {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot start level-server: {0}")]
    Spawn(std::io::Error),
    #[error("level-server did not come up at {0} (see its log)")]
    StartTimeout(PathBuf),
    #[error("connection error: {0}")]
    Io(#[from] std::io::Error),
    #[error("the server closed the connection")]
    Closed,
    #[error("no reply from the server")]
    ReplyTimeout,
    #[error("protocol error: {0}")]
    Protocol(String),
}

impl From<ClientError> for CommandError {
    fn from(e: ClientError) -> Self {
        match e {
            ClientError::Protocol(message) => CommandError::Protocol { message },
            other => CommandError::ServerUnavailable {
                message: other.to_string(),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum ServerEvent {
    Snapshot(Box<SnapshotEvent>),
    Changed(Box<ChangedEvent>),
    Closed,
}

#[derive(Debug, Clone)]
pub struct Reply {
    pub response: Response,
    pub png: Option<Vec<u8>>,
}

impl Reply {
    pub fn is_ok(&self) -> bool {
        self.response.is_ok()
    }

    pub fn error(&self) -> Option<&ErrorResponse> {
        match &self.response {
            Response::Error(e) => Some(e),
            Response::Success(_) => None,
        }
    }

    pub fn value(&self) -> Value {
        self.response.to_value()
    }

    /// Decodes a successful reply's `result`; an error reply becomes the matching `CommandError`.
    pub fn result<R: DeserializeOwned>(&self) -> Result<R, ClientError> {
        match &self.response {
            Response::Success(s) => serde_json::from_value(s.result.clone())
                .map_err(|e| ClientError::Protocol(format!("unexpected result shape: {e}"))),
            Response::Error(e) => Err(ClientError::Protocol(format!("{}: {}", e.code.as_str(), e.message))),
        }
    }
}

pub type WakeFn = Box<dyn Fn() + Send + Sync>;

type Pending = Arc<Mutex<HashMap<u64, Sender<Reply>>>>;

pub struct Client {
    writer: Mutex<UnixStream>,
    pending: Pending,
    events: Mutex<Receiver<ServerEvent>>,
    next_id: AtomicU64,
    conn: u64,
    discard: Arc<AtomicBool>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn server_binary(config: &Config) -> PathBuf {
    if let Some(binary) = &config.server.binary {
        return binary.clone();
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let sibling = dir.join("level-server");
        if sibling.exists() {
            return sibling;
        }
    }
    PathBuf::from("level-server")
}

fn spawn_server(config: &Config) -> Result<(), ClientError> {
    let log_path = config.log_path();
    if let Some(dir) = log_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(ClientError::Spawn)?;
    let err = log.try_clone().map_err(ClientError::Spawn)?;
    let mut cmd = Command::new(server_binary(config));
    if let Some(source) = config.source() {
        cmd.arg("--config").arg(source);
    }
    cmd.stdin(Stdio::null())
        .stdout(log)
        .stderr(err)
        .process_group(0)
        .spawn()
        .map_err(ClientError::Spawn)?;
    Ok(())
}

fn connect_socket(config: &Config, autospawn: bool) -> Result<UnixStream, ClientError> {
    let path = config.socket_path();
    let first = UnixStream::connect(&path);
    let err = match first {
        Ok(stream) => return Ok(stream),
        Err(e) => e,
    };
    if !autospawn {
        return Err(ClientError::Connect { path, source: err });
    }
    spawn_server(config)?;
    let deadline = Instant::now() + SPAWN_WAIT;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        if let Ok(stream) = UnixStream::connect(&path) {
            return Ok(stream);
        }
    }
    Err(ClientError::StartTimeout(path))
}

fn read_loop(
    stream: UnixStream,
    pending: Pending,
    events: Sender<ServerEvent>,
    wake: Option<WakeFn>,
    discard: Arc<AtomicBool>,
) {
    let notify = || {
        if let Some(wake) = &wake {
            wake();
        }
    };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let msg = match serde_json::from_str::<ServerMsg>(&line) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("level-client: unreadable server message: {e}");
                continue;
            }
        };
        match msg {
            ServerMsg::Reply {
                id,
                response,
                png_base64,
            } => {
                let png = png_base64.and_then(|b| {
                    base64::engine::general_purpose::STANDARD.decode(b).ok()
                });
                if let Some(tx) = lock(&pending).remove(&id) {
                    let _ = tx.send(Reply { response, png });
                }
            }
            ServerMsg::Snapshot(_) | ServerMsg::Changed(_) if discard.load(Ordering::Relaxed) => {}
            ServerMsg::Snapshot(s) => {
                let _ = events.send(ServerEvent::Snapshot(Box::new(s)));
                notify();
            }
            ServerMsg::Changed(c) => {
                let _ = events.send(ServerEvent::Changed(Box::new(c)));
                notify();
            }
        }
    }
    lock(&pending).clear();
    let _ = events.send(ServerEvent::Closed);
    notify();
}

impl Client {
    pub fn connect(
        config: &Config,
        client: &str,
        mutate: bool,
        destructive: bool,
    ) -> Result<Self, ClientError> {
        Self::connect_with(config, client, mutate, destructive, true, None)
    }

    pub fn connect_with(
        config: &Config,
        client: &str,
        mutate: bool,
        destructive: bool,
        autospawn: bool,
        wake: Option<WakeFn>,
    ) -> Result<Self, ClientError> {
        let stream = connect_socket(config, autospawn)?;
        let reader = stream.try_clone()?;
        let pending: Pending = Arc::default();
        let (tx, events) = mpsc::channel();
        let thread_pending = pending.clone();
        let discard = Arc::new(AtomicBool::new(false));
        let thread_discard = discard.clone();
        std::thread::Builder::new()
            .name("level-client-reader".into())
            .spawn(move || read_loop(reader, thread_pending, tx, wake, thread_discard))?;
        let mut me = Self {
            writer: Mutex::new(stream),
            pending,
            events: Mutex::new(events),
            next_id: AtomicU64::new(1),
            conn: 0,
            discard,
        };
        let hello = ClientMsg::Hello {
            id: 0,
            protocol: PROTOCOL,
            client: client.to_string(),
            mutate,
            destructive,
            cwd: std::env::current_dir()
                .ok()
                .map(|p| p.display().to_string()),
        };
        let reply = me.roundtrip(0, &hello)?;
        if let Some(e) = reply.error() {
            return Err(ClientError::Protocol(e.message.clone()));
        }
        me.conn = reply.result::<HelloResult>()?.conn;
        Ok(me)
    }

    /// Stops queuing change events, for clients that never read them.
    pub fn discard_events(&self) {
        self.discard.store(true, Ordering::Relaxed);
        while self.try_event().is_some() {}
    }

    pub fn conn(&self) -> u64 {
        self.conn
    }

    pub fn try_event(&self) -> Option<ServerEvent> {
        lock(&self.events).try_recv().ok()
    }

    pub fn next_event(&self, timeout: Duration) -> Option<ServerEvent> {
        lock(&self.events).recv_timeout(timeout).ok()
    }

    fn roundtrip(&self, id: u64, msg: &ClientMsg) -> Result<Reply, ClientError> {
        let (tx, rx) = mpsc::channel();
        lock(&self.pending).insert(id, tx);
        let mut line = serde_json::to_string(msg).expect("messages serialize");
        line.push('\n');
        {
            let mut writer = lock(&self.writer);
            if let Err(e) = writer.write_all(line.as_bytes()).and_then(|()| writer.flush()) {
                lock(&self.pending).remove(&id);
                return Err(ClientError::Io(e));
            }
        }
        match rx.recv_timeout(REPLY_WAIT) {
            Ok(reply) => Ok(reply),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                lock(&self.pending).remove(&id);
                Err(ClientError::ReplyTimeout)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(ClientError::Closed),
        }
    }

    pub fn request(&self, request: Request) -> Result<Reply, ClientError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.roundtrip(id, &ClientMsg::Request { id, request })
    }

    pub fn control(&self, action: ControlAction, force: bool) -> Result<Reply, ClientError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.roundtrip(
            id,
            &ClientMsg::Control {
                id,
                action,
                force,
            },
        )
    }

    pub fn request_value(&self, request: Value) -> Result<Reply, ClientError> {
        let request = Request::from_value(request)
            .map_err(|e| ClientError::Protocol(e.to_string()))?;
        self.request(request)
    }
}

pub fn socket_is_live(path: &Path) -> bool {
    UnixStream::connect(path).is_ok()
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = lock(&self.writer).shutdown(std::net::Shutdown::Both);
    }
}
