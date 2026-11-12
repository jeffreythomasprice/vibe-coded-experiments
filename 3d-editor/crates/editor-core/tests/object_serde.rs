use editor_core::*;

fn roundtrip(object: Object) {
    let json = serde_json::to_string(&object).unwrap();
    let back: Object = serde_json::from_str(&json).unwrap();
    assert_eq!(object, back, "{json}");
}

fn layer() -> LayerId {
    LayerId(1)
}

fn extra() -> Extra {
    let mut e = Extra::new();
    e.insert("note".into(), toml::Value::String("kept".into()));
    e
}

#[test]
fn brush_roundtrips() {
    let face = |id, points| BrushFace {
        id: FaceId(id),
        points,
        attrs: FaceAttrs::with_material("wall"),
    };
    roundtrip(Object::Brush(BrushObject {
        flags: Default::default(),
        id: BrushId(2),
        layer: layer(),
        group: Some(GroupId(3)),
        entity: None,
        brush: Brush {
            faces: vec![face(1, [DVec3::new(0.0, 0.0, 0.0), DVec3::X, DVec3::Y])],
        },
        extra: extra(),
    }));
}

#[test]
fn mesh_roundtrips() {
    roundtrip(Object::Mesh(MeshObject {
        flags: Default::default(),
        id: MeshId(4),
        layer: layer(),
        group: None,
        entity: Some(EntityId(5)),
        mesh: Mesh {
            vertices: vec![
                MeshVertex {
                    id: VertexId(1),
                    pos: DVec3::ZERO,
                },
                MeshVertex {
                    id: VertexId(2),
                    pos: DVec3::X,
                },
                MeshVertex {
                    id: VertexId(3),
                    pos: DVec3::Y,
                },
            ],
            faces: vec![MeshFace {
                id: FaceId(1),
                verts: vec![VertexId(1), VertexId(2), VertexId(3)],
                attrs: FaceAttrs::default(),
            }],
        },
        extra: Extra::new(),
    }));
}

#[test]
fn entity_roundtrips() {
    let data = match serde_json::json!({
        "light": 300,
        "on": true,
        "tint": [1.0, 0.5, 0.0],
        "keys": [{ "time": 0, "offset": [0, 0, 64] }],
        "link": "e9",
    }) {
        serde_json::Value::Object(map) => map,
        _ => unreachable!(),
    };
    roundtrip(Object::Entity(Entity {
        flags: Default::default(),
        id: EntityId(5),
        layer: layer(),
        group: None,
        origin: DVec3::new(8.0, 16.0, 24.0),
        angles: DVec3::new(0.0, 90.0, 0.0),
        components: [
            ("light".to_string(), data.clone()),
            ("empty".to_string(), Default::default()),
        ]
        .into(),
        extra: extra(),
    }));
}

#[test]
fn group_layer_camera_roundtrip() {
    roundtrip(Object::Group(Group {
        flags: Default::default(),
        id: GroupId(3),
        name: "hall".into(),
        layer: layer(),
        parent: Some(GroupId(2)),
        extra: Extra::new(),
    }));
    roundtrip(Object::Layer(Layer {
        flags: Default::default(),
        omit_from_export: false,
        id: layer(),
        name: "main".into(),
        extra: extra(),
    }));
    for projection in [
        Projection::Perspective { fov: 70.0 },
        Projection::Orthographic {
            view: OrthoView::Top,
            zoom: 2.5,
        },
    ] {
        roundtrip(Object::Camera(Camera {
            id: CameraId(6),
            name: "cam".into(),
            position: DVec3::new(1.0, 2.0, 3.0),
            yaw: 45.0,
            pitch: -10.0,
            projection,
            extra: Extra::new(),
        }));
    }
}

#[test]
fn find_null_names_the_pointer() {
    let v = serde_json::json!({ "a": [1, { "b/c": null }] });
    assert_eq!(find_null(&v).as_deref(), Some("/a/1/b~1c"));
    assert_eq!(find_null(&serde_json::json!({ "a": [1, 2] })), None);
}

#[test]
fn component_defs_roundtrip() {
    roundtrip(Object::ComponentDefs(ComponentDefs {
        id: ComponentDefsId(4),
        source: DefsSource::Embedded {
            defs: ComponentDefsFile {
                version: DEFS_VERSION,
                components: vec![ComponentDef {
                    name: "light".parse().unwrap(),
                    description: None,
                    category: Some("lights".into()),
                    color: Some([1.0, 0.5, 0.0]),
                    size: None,
                    schema: serde_json::json!({ "type": "object" }),
                }],
            },
        },
        extra: Extra::new(),
    }));
    roundtrip(Object::ComponentDefs(ComponentDefs {
        id: ComponentDefsId(5),
        source: DefsSource::Path {
            path: "defs/game.components.json".into(),
        },
        extra: Extra::new(),
    }));
    assert_eq!("d4".parse::<ObjectId>().unwrap(), ComponentDefsId(4).into());
}
