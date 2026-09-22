//! A minimal stand-in for `sd-server`, used only by `tests/sd_server.rs`
//! (behind the `sd-server-tests` feature) to exercise `sd::daemon`'s process
//! lifecycle — spawn, readiness polling, kill/respawn, port-conflict
//! detection — without needing the real upstream binary or real model
//! weights. It understands exactly one route, `GET /sdcpp/v1/capabilities`,
//! parsing just enough of `--listen-ip`/`--listen-port` from argv to bind
//! where `sd::daemon` expects.

use std::io::{Read, Write};
use std::net::TcpListener;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ip = flag_value(&args, "--listen-ip").unwrap_or_else(|| "127.0.0.1".to_owned());
    let port = flag_value(&args, "--listen-port").unwrap_or_else(|| "1234".to_owned());

    let listener = TcpListener::bind(format!("{ip}:{port}")).expect("fake-sd-server: bind failed");

    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);

        let body = r#"{"model":{"name":"fake","stem":"fake","path":"/fake/model.gguf"},"current_mode":"img_gen"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
    }
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}
