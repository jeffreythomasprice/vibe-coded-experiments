#![cfg(feature = "sd-server-tests")]

//! Exercises `sd::daemon`'s process lifecycle against a real spawned process —
//! `tests/fixtures/fake_sd_server.rs`, not the real upstream binary, so this
//! suite needs no download and no model weights. It proves the mechanics
//! (spawn, readiness polling, dead-process respawn, mutual exclusion under
//! concurrency); correctness of the real `sd-server` binary itself is
//! upstream's responsibility. Run with `cargo test --features sd-server-tests`.

use std::path::PathBuf;
use std::time::Duration;

use image_gen::sd::config::SdConfig;
use image_gen::sd::daemon;
use image_gen::sd::modelset::ModelSet;

const FAKE_MODEL_PATH: &str = "/fake/model.gguf";

fn install_fake_binary(server_dir: &std::path::Path, tag: &str) {
    let dest_dir = server_dir.join(tag);
    std::fs::create_dir_all(&dest_dir).unwrap();
    let dest = dest_dir.join("sd-server");
    std::fs::copy(env!("CARGO_BIN_EXE_fake-sd-server"), &dest).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn test_config(dir: &std::path::Path, port: u16, tag: &str) -> SdConfig {
    SdConfig {
        port,
        release_tag: tag.to_owned(),
        server_dir: dir.join("server"),
        state_dir: dir.join("run"),
        startup_timeout_secs: 10,
        poll_interval_ms: 20,
        ..SdConfig::default()
    }
}

fn model_set(tag: &str) -> ModelSet {
    ModelSet {
        model: None,
        diffusion_model: Some(PathBuf::from(FAKE_MODEL_PATH)),
        vae: None,
        clip_l: None,
        clip_g: None,
        t5xxl: None,
        taesd: None,
        text_encoder: None,
        vision_encoder: None,
        weight_type: None,
        threads: None,
        backend: None,
        vae_tiling: false,
        flash_attn: false,
        release_tag: tag.to_owned(),
    }
}

#[tokio::test]
async fn ensure_ready_spawns_and_becomes_reachable() {
    let dir = tempfile::tempdir().unwrap();
    let tag = "fake-tag-spawn";
    install_fake_binary(&dir.path().join("server"), tag);
    let config = test_config(dir.path(), free_port(), tag);
    let set = model_set(tag);

    let client = daemon::ensure_ready(&config, &dir.path().join("logs"), &set)
        .await
        .unwrap();
    let caps = client.capabilities().await.unwrap();
    assert_eq!(caps.model.path, FAKE_MODEL_PATH);

    let status = daemon::status(&config).await.unwrap();
    assert!(status.alive);
    assert_eq!(status.state.fingerprint, set.fingerprint());
}

#[tokio::test]
async fn second_call_with_the_same_model_set_reuses_the_process() {
    let dir = tempfile::tempdir().unwrap();
    let tag = "fake-tag-reuse";
    install_fake_binary(&dir.path().join("server"), tag);
    let config = test_config(dir.path(), free_port(), tag);
    let set = model_set(tag);
    let log_dir = dir.path().join("logs");

    daemon::ensure_ready(&config, &log_dir, &set).await.unwrap();
    let first_pid = daemon::status(&config).await.unwrap().state.pid;

    daemon::ensure_ready(&config, &log_dir, &set).await.unwrap();
    let second_pid = daemon::status(&config).await.unwrap().state.pid;

    assert_eq!(first_pid, second_pid);
}

#[tokio::test]
async fn a_killed_process_is_respawned() {
    let dir = tempfile::tempdir().unwrap();
    let tag = "fake-tag-respawn";
    install_fake_binary(&dir.path().join("server"), tag);
    let config = test_config(dir.path(), free_port(), tag);
    let set = model_set(tag);
    let log_dir = dir.path().join("logs");

    daemon::ensure_ready(&config, &log_dir, &set).await.unwrap();
    let first_pid = daemon::status(&config).await.unwrap().state.pid;

    unsafe {
        libc::kill(first_pid as libc::pid_t, libc::SIGKILL);
    }
    // Give the OS a moment to reap the process and release the port.
    tokio::time::sleep(Duration::from_millis(300)).await;

    daemon::ensure_ready(&config, &log_dir, &set).await.unwrap();
    let status = daemon::status(&config).await.unwrap();
    assert!(status.alive);
    assert_ne!(status.state.pid, first_pid);
}

#[tokio::test]
async fn stop_terminates_the_process() {
    let dir = tempfile::tempdir().unwrap();
    let tag = "fake-tag-stop";
    install_fake_binary(&dir.path().join("server"), tag);
    let config = test_config(dir.path(), free_port(), tag);
    let set = model_set(tag);

    daemon::ensure_ready(&config, &dir.path().join("logs"), &set)
        .await
        .unwrap();
    assert!(daemon::stop(&config).await.unwrap());

    tokio::time::sleep(Duration::from_millis(200)).await;
    let status = daemon::status(&config).await;
    assert!(status.is_none(), "stop should remove the state file");
}

#[tokio::test]
async fn concurrent_ensure_ready_calls_spawn_exactly_one_process() {
    let dir = tempfile::tempdir().unwrap();
    let tag = "fake-tag-concurrent";
    install_fake_binary(&dir.path().join("server"), tag);
    let config = test_config(dir.path(), free_port(), tag);
    let set = model_set(tag);
    let log_dir = dir.path().join("logs");

    // If the lock ever let two spawns race, the loser's fake server would
    // panic on its own `TcpListener::bind` (the winner already holds the
    // port), and `spawn_and_wait` would surface that as `ServerExited` —
    // so both calls succeeding at all is itself the proof of exclusivity.
    let (a, b) = tokio::join!(
        daemon::ensure_ready(&config, &log_dir, &set),
        daemon::ensure_ready(&config, &log_dir, &set),
    );
    a.unwrap();
    b.unwrap();

    let status = daemon::status(&config).await.unwrap();
    assert!(status.alive);
}
