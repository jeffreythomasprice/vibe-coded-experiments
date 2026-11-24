use std::{fs, path::Path};

use audio_recorder_schema::{
    Schema,
    analysis::Analysis, SchemaError,
    config::Config,
    device_state::DeviceStateFile,
    from_json_str,
    ipc::{IpcError, LiveTranscriptEventDto, PlayArgs, RecorderEventDto},
    manifest::Manifest,
    mix::MixSettings,
    settings::Settings,
    theme::Theme,
    to_json_string_pretty,
};

fn fixtures(dir: &str, prefix: &str) -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(dir);
    let mut out: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(prefix))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), fs::read_to_string(p).unwrap()))
        .collect();
    out.sort();
    assert!(!out.is_empty());
    out
}

#[test]
fn valid_config_fixtures_round_trip() {
    for (name, text) in fixtures("config", "valid-") {
        let cfg: Config = from_json_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        let again = to_json_string_pretty(&cfg).unwrap();
        let reparsed: Config = from_json_str(&again).unwrap();
        assert_eq!(serde_json::to_value(&cfg).unwrap(), serde_json::to_value(&reparsed).unwrap(), "{name}");
    }
}

#[test]
fn invalid_config_fixtures_are_rejected_by_validation() {
    for (name, text) in fixtures("config", "invalid-") {
        let err = from_json_str::<Config>(&text).unwrap_err();
        assert!(matches!(err, SchemaError::Validation { .. }), "{name}: {err}");
    }
}

#[test]
fn validation_reports_instance_path() {
    let err = from_json_str::<Config>(r#"{ "sample_format": "mp3" }"#).unwrap_err();
    let SchemaError::Validation { errors, .. } = err else { panic!("expected validation error") };
    assert_eq!(errors[0].instance_path, "/sample_format");
}

#[test]
fn defaults_apply() {
    let cfg: Config = from_json_str("{}").unwrap();
    assert!(cfg.sessions_dir.is_none());
    assert_eq!(serde_json::to_value(&cfg).unwrap()["sample_format"], "f32");
}

fn check_fixtures<T: Schema + std::fmt::Debug>(dir: &str) {
    for (name, text) in fixtures(dir, "valid-") {
        let v: T = from_json_str(&text).unwrap_or_else(|e| panic!("{dir}/{name}: {e}"));
        let again = to_json_string_pretty(&v).unwrap_or_else(|e| panic!("{dir}/{name} re-serialize: {e}"));
        let reparsed: T = from_json_str(&again).unwrap();
        assert_eq!(serde_json::to_value(&v).unwrap(), serde_json::to_value(&reparsed).unwrap(), "{dir}/{name}");
        let original: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(serde_json::to_value(&v).unwrap(), original, "{dir}/{name} lossless");
    }
    for (name, text) in fixtures(dir, "invalid-") {
        let err = from_json_str::<T>(&text).unwrap_err();
        assert!(matches!(err, SchemaError::Validation { .. }), "{dir}/{name}: {err}");
    }
}

#[test]
fn device_state_fixtures() {
    check_fixtures::<DeviceStateFile>("device-state");
}

#[test]
fn manifest_fixtures() {
    check_fixtures::<Manifest>("manifest");
}

#[test]
fn ipc_event_fixtures() {
    check_fixtures::<RecorderEventDto>("ipc");
}

#[test]
fn ipc_live_event_fixtures() {
    check_fixtures::<LiveTranscriptEventDto>("ipc-live");
}

#[test]
fn ipc_nullable_fields_and_error_type() {
    let args: PlayArgs = from_json_str(r#"{ "name": "s", "track": null }"#).unwrap();
    assert!(args.track.is_none());
    assert!(from_json_str::<PlayArgs>(r#"{ "name": "s" }"#).is_err());
    assert_eq!(IpcError::NAME, "ipc.IpcError");
    assert!(from_json_str::<IpcError>(r#"{ "kind": "nope", "message": "m" }"#).is_err());
}

#[test]
fn theme_fixtures() {
    check_fixtures::<Theme>("theme");
}

#[test]
fn settings_fixtures() {
    check_fixtures::<Settings>("settings");
}

#[test]
fn mix_fixtures() {
    check_fixtures::<MixSettings>("mix");
}

#[test]
fn analysis_fixtures() {
    check_fixtures::<Analysis>("analysis");
}

#[test]
fn transcript_fixtures() {
    check_fixtures::<audio_recorder_schema::transcript::Transcript>("transcript");
}

#[test]
fn jobs_fixtures() {
    check_fixtures::<audio_recorder_schema::jobs::Job>("jobs");
}

#[test]
fn summary_fixtures() {
    check_fixtures::<audio_recorder_schema::summary::Summary>("summary");
}

#[test]
fn llm_wire_formats() {
    use audio_recorder_schema::llm::{AnthropicResponse, OllamaChatResponse};
    let a: AnthropicResponse = from_json_str(
        r#"{ "id": "msg_1", "content": [ { "type": "thinking", "thinking": "" }, { "type": "text", "text": "hi" } ], "stop_reason": "end_turn", "usage": {} }"#,
    )
    .unwrap();
    assert_eq!(a.content.len(), 2);
    assert!(a.content[0].text.is_none());
    let o: OllamaChatResponse =
        from_json_str(r#"{ "model": "m", "message": { "role": "assistant", "content": "hi" }, "done": true }"#).unwrap();
    assert_eq!(o.message.content, "hi");
    assert!(from_json_str::<OllamaChatResponse>(r#"{ "done": true }"#).is_err());
}
