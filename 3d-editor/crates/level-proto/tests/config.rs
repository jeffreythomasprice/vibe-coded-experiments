use level_proto::{Config, ConfigError};

fn write(tag: &str, text: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("level-proto-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn explicit_values_override_defaults() {
    let path = write(
        "values",
        "socket = \"/tmp/x.sock\"\n[server]\nidle_exit_secs = 9\n[mcp]\nallow_mutate = true\n",
    );
    let c = Config::load_from(&path).unwrap();
    assert_eq!(c.socket_path(), std::path::Path::new("/tmp/x.sock"));
    assert_eq!(c.lock_path(), std::path::Path::new("/tmp/x.sock.lock"));
    assert_eq!(c.server.idle_exit_secs, 9);
    assert!(c.mcp.allow_mutate && !c.mcp.allow_destructive);
    assert_eq!(c.source(), Some(path.as_path()));
}

#[test]
fn unknown_keys_and_long_sockets_are_errors() {
    let path = write("unknown", "sokcet = \"/tmp/x.sock\"\n");
    assert!(matches!(Config::load_from(&path), Err(ConfigError::Parse { .. })));

    let long = format!("socket = \"/tmp/{}/s.sock\"\n", "d".repeat(120));
    let path = write("long", &long);
    assert!(matches!(
        Config::load_from(&path),
        Err(ConfigError::SocketPathTooLong { .. })
    ));

    let missing = std::path::Path::new("/nonexistent/config.toml");
    assert!(matches!(Config::load_from(missing), Err(ConfigError::Read { .. })));
}
