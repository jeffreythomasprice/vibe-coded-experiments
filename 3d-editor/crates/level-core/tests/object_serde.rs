use level_core::*;

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
        id: BrushId(2),
        layer: layer(),
        group: Some(GroupId(3)),
        entity: None,
        brush: Brush {
            faces: vec![face(
                1,
                [DVec3::new(0.0, 0.0, 0.0), DVec3::X, DVec3::Y],
            )],
        },
        extra: extra(),
    }));
}

#[test]
fn mesh_roundtrips() {
    roundtrip(Object::Mesh(MeshObject {
        id: MeshId(4),
        layer: layer(),
        group: None,
        entity: Some(EntityId(5)),
        mesh: Mesh {
            vertices: vec![
                MeshVertex { id: VertexId(1), pos: DVec3::ZERO },
                MeshVertex { id: VertexId(2), pos: DVec3::X },
                MeshVertex { id: VertexId(3), pos: DVec3::Y },
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
    let mut properties = Properties::new();
    properties.insert("light".into(), PropertyValue::Number(300.0));
    properties.insert("on".into(), PropertyValue::Bool(true));
    properties.insert("tint".into(), PropertyValue::Vec3(DVec3::new(1.0, 0.5, 0.0)));
    properties.insert("target".into(), PropertyValue::String("door".into()));
    for origin in [Some(DVec3::new(8.0, 16.0, 24.0)), None] {
        roundtrip(Object::Entity(Entity {
            id: EntityId(5),
            classname: "light".into(),
            layer: layer(),
            group: None,
            origin,
            angles: DVec3::new(0.0, 90.0, 0.0),
            properties: properties.clone(),
            extra: extra(),
        }));
    }
}

#[test]
fn group_layer_camera_roundtrip() {
    roundtrip(Object::Group(Group {
        id: GroupId(3),
        name: "hall".into(),
        layer: layer(),
        parent: Some(GroupId(2)),
        extra: Extra::new(),
    }));
    roundtrip(Object::Layer(Layer {
        id: layer(),
        name: "main".into(),
        extra: extra(),
    }));
    for projection in [
        Projection::Perspective { fov: 70.0 },
        Projection::Orthographic { view: OrthoView::Top, zoom: 2.5 },
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
