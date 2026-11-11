use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use base64::Engine;
use editor_cmd::{Changes, CommandError, Response, Success};
use editor_host::{ConnId, Hub, Policy};
use editor_proto::{ClientMsg, Config, HelloResult, PROTOCOL, ServerMsg};

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

fn io(path: &std::path::Path) -> impl FnOnce(std::io::Error) -> ServeError + '_ {
    |source| ServeError::Io {
        path: path.to_path_buf(),
        source,
    }
}

struct State {
    hub: Hub,
    senders: HashMap<ConnId, Sender<String>>,
    stopping: bool,
}

type Shared = Arc<Mutex<State>>;

fn lock(shared: &Shared) -> MutexGuard<'_, State> {
    shared.lock().unwrap_or_else(|p| p.into_inner())
}

fn encode(msg: &ServerMsg) -> String {
    let mut line = serde_json::to_string(msg).expect("messages serialize");
    line.push('\n');
    line
}

fn reply_line(id: u64, response: Response, png: Option<Vec<u8>>) -> String {
    encode(&ServerMsg::Reply {
        id,
        response,
        png_base64: png.map(|p| base64::engine::general_purpose::STANDARD.encode(p)),
    })
}

fn deliver(state: &State, events: Vec<(ConnId, ServerMsg)>) {
    for (conn, msg) in events {
        if let Some(tx) = state.senders.get(&conn) {
            let _ = tx.send(encode(&msg));
        }
    }
}

fn writer_thread(stream: UnixStream, rx: mpsc::Receiver<String>) {
    let mut stream = stream;
    for line in rx {
        if stream
            .write_all(line.as_bytes())
            .and_then(|()| stream.flush())
            .is_err()
        {
            break;
        }
    }
}

fn handle_connection(shared: Shared, stream: UnixStream) {
    let Ok(write_half) = stream.try_clone() else {
        return;
    };
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || writer_thread(write_half, rx));

    let mut conn: Option<ConnId> = None;
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let msg = match serde_json::from_str::<ClientMsg>(&line) {
            Ok(m) => m,
            Err(e) => {
                let err = CommandError::Protocol {
                    message: format!("bad message: {e}"),
                };
                let _ = tx.send(reply_line(0, Response::error(&err), None));
                continue;
            }
        };
        match msg {
            ClientMsg::Hello {
                id,
                protocol,
                client,
                mutate,
                destructive,
                cwd,
            } => {
                let mut state = lock(&shared);
                if conn.is_some() {
                    continue;
                }
                match state
                    .hub
                    .connect(&client, protocol, Policy::new(mutate, destructive))
                {
                    Ok(c) => {
                        state.hub.set_cwd(c, cwd.map(PathBuf::from));
                        conn = Some(c);
                        state.senders.insert(c, tx.clone());
                        eprintln!("connect {c} ({client})");
                        let hello = HelloResult {
                            conn: c,
                            protocol: PROTOCOL,
                        };
                        let ok = Response::Success(Success {
                            ok: true,
                            revision: 0,
                            result: serde_json::to_value(hello).expect("hello serializes"),
                            changes: Changes::default(),
                            warnings: vec![],
                        });
                        let _ = tx.send(reply_line(id, ok, None));
                    }
                    Err(e) => {
                        let _ = tx.send(reply_line(id, Response::error(&e), None));
                        break;
                    }
                }
            }
            ClientMsg::Request { id, request } => {
                let Some(c) = conn else {
                    let err = CommandError::Protocol {
                        message: "send hello first".into(),
                    };
                    let _ = tx.send(reply_line(id, Response::error(&err), None));
                    continue;
                };
                let mut state = lock(&shared);
                let handled = state.hub.handle(c, request);
                deliver(&state, handled.events);
                let _ = tx.send(reply_line(id, handled.reply.response, handled.reply.png));
            }
            ClientMsg::Control { id, action, force } => {
                let Some(c) = conn else {
                    let err = CommandError::Protocol {
                        message: "send hello first".into(),
                    };
                    let _ = tx.send(reply_line(id, Response::error(&err), None));
                    continue;
                };
                let mut state = lock(&shared);
                let (response, stop) = state.hub.control(c, action, force);
                state.stopping |= stop;
                let _ = tx.send(reply_line(id, response, None));
            }
        }
    }
    if let Some(c) = conn {
        let mut state = lock(&shared);
        let events = state.hub.disconnect(c);
        state.senders.remove(&c);
        deliver(&state, events);
        eprintln!("disconnect {c}");
    }
}

fn watch_idle(shared: Shared, config: Config) {
    let limit = Duration::from_secs(config.server.idle_exit_secs);
    let mut idle_since = Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let (stopping, idle) = {
            let state = lock(&shared);
            (
                state.stopping,
                state.hub.conn_count() == 0 && state.hub.dirty_sessions() == 0,
            )
        };
        if !idle {
            idle_since = Instant::now();
        }
        if stopping || idle_since.elapsed() >= limit {
            if stopping {
                std::thread::sleep(Duration::from_millis(150));
            }
            let _ = std::fs::remove_file(config.socket_path());
            eprintln!("exiting ({})", if stopping { "stopped" } else { "idle" });
            std::process::exit(0);
        }
    }
}

pub fn run(config: &Config) -> Result<(), ServeError> {
    let socket = config.socket_path();
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir).map_err(io(dir))?;
    }
    let lock_path = config.lock_path();
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .map_err(io(&lock_path))?;
    if lock_file.try_lock().is_err() {
        return Ok(());
    }
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).map_err(io(&socket))?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
        .map_err(io(&socket))?;
    eprintln!("listening on {}", socket.display());

    let shared: Shared = Arc::new(Mutex::new(State {
        hub: Hub::new(),
        senders: HashMap::new(),
        stopping: false,
    }));
    {
        let shared = shared.clone();
        let config = config.clone();
        std::thread::spawn(move || watch_idle(shared, config));
    }
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let shared = shared.clone();
        std::thread::spawn(move || handle_connection(shared, stream));
    }
    drop(lock_file);
    Ok(())
}
