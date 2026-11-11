use editor_cmd::{Request, Response};
use editor_proto::{
    ChangedEvent, ClientMsg, ControlAction, FileInfo, HistoryInfo, ServerMsg, SnapshotEvent,
    wire_schema,
};
use serde_json::{Value, json};

fn validator(root: &str) -> jsonschema::Validator {
    jsonschema::validator_for(&wire_schema()[root]).expect("schema compiles")
}

fn check(root: &str, value: &Value) {
    let v = validator(root);
    let errors: Vec<String> = v.iter_errors(value).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{root}: {errors:?}\n{value}");
}

fn client_samples() -> Vec<ClientMsg> {
    vec![
        ClientMsg::Hello {
            id: 0,
            protocol: 2,
            client: "t".into(),
            mutate: true,
            destructive: false,
            cwd: Some("/tmp".into()),
        },
        ClientMsg::Request {
            id: 1,
            request: Request::new("scene.summary", json!({})),
        },
        ClientMsg::Control {
            id: 2,
            action: ControlAction::Stop,
            force: true,
        },
    ]
}

fn server_samples() -> Vec<ServerMsg> {
    let file = FileInfo {
        path: None,
        name: "x".into(),
        dirty: false,
    };
    let history = HistoryInfo {
        can_undo: false,
        can_redo: false,
        transaction: None,
    };
    vec![
        ServerMsg::Reply {
            id: 1,
            response: Response::error(&editor_cmd::CommandError::NoTransaction),
            png_base64: None,
        },
        ServerMsg::Snapshot(SnapshotEvent {
            session: "s1".into(),
            revision: 3,
            document_toml: String::new(),
            selection: vec![],
            file: file.clone(),
            history: history.clone(),
        }),
        ServerMsg::Changed(ChangedEvent {
            session: "s1".into(),
            revision: 4,
            upsert: vec![],
            deleted: vec![],
            selection: Some(vec![]),
            file: Some(file),
            history: Some(history),
        }),
    ]
}

#[test]
fn messages_round_trip_and_match_the_schema() {
    for msg in client_samples() {
        let value = serde_json::to_value(&msg).unwrap();
        check("client_msg", &value);
        assert_eq!(serde_json::from_value::<ClientMsg>(value).unwrap(), msg);
    }
    for msg in server_samples() {
        let value = serde_json::to_value(&msg).unwrap();
        check("server_msg", &value);
        assert_eq!(serde_json::from_value::<ServerMsg>(value).unwrap(), msg);
    }
}

#[test]
fn unknown_fields_and_actions_are_rejected() {
    let bad = json!({ "type": "control", "id": 1, "action": "reboot" });
    assert!(serde_json::from_value::<ClientMsg>(bad.clone()).is_err());
    assert!(!validator("client_msg").is_valid(&bad));

    let extra = json!({ "type": "control", "id": 1, "action": "stop", "surprise": true });
    assert!(serde_json::from_value::<ClientMsg>(extra.clone()).is_err());
    assert!(!validator("client_msg").is_valid(&extra));
}
