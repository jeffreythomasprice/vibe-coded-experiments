use editor_core::*;
use editor_fmt::{read, write};
use editor_geom::shapes::cuboid;
use proptest::prelude::*;

#[derive(Debug, Clone)]
struct Spec {
    min: [i32; 3],
    size: [i32; 3],
    material: String,
    offset: (f64, f64),
    scale: (f64, f64),
    rotation: f64,
    props: Vec<(String, PropValue)>,
    classname: String,
    origin: Option<[f64; 3]>,
    camera: Option<(f64, f64, f64, bool)>,
    nest: bool,
}

#[derive(Debug, Clone)]
enum PropValue {
    Bool(bool),
    Num(f64),
    Vec([f64; 3]),
    Str(String),
}

fn float() -> impl Strategy<Value = f64> {
    prop_oneof![
        (-1000i32..1000).prop_map(|i| i as f64),
        (-1000.0f64..1000.0),
        (-8000i32..8000).prop_map(|i| i as f64 / 8.0),
        Just(1e-9),
        Just(1.5e300),
    ]
}

fn text() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z/_]{0,12}",
        "[ -~]{0,12}",
        Just("quote\"back\\slash\nnewline\ttab é ✓".to_string()),
    ]
}

fn spec() -> impl Strategy<Value = Spec> {
    (
        prop::array::uniform3(-64i32..64),
        prop::array::uniform3(1i32..64),
        text(),
        (float(), float()),
        (float(), float()),
        float(),
        prop::collection::vec(
            (
                "[a-z_][a-z_0-9]{0,6}|[ -~]{1,6}",
                prop_oneof![
                    any::<bool>().prop_map(PropValue::Bool),
                    float().prop_map(PropValue::Num),
                    prop::array::uniform3(float()).prop_map(PropValue::Vec),
                    text().prop_map(PropValue::Str),
                ],
            ),
            0..4,
        ),
        "[a-z_]{1,10}",
        prop::option::of(prop::array::uniform3(float())),
        prop::option::of((float(), float(), 1.0f64..170.0, any::<bool>())),
        any::<bool>(),
    )
        .prop_map(
            |(
                min,
                size,
                material,
                offset,
                scale,
                rotation,
                props,
                classname,
                origin,
                camera,
                nest,
            )| {
                Spec {
                    min,
                    size,
                    material,
                    offset,
                    scale,
                    rotation,
                    props,
                    classname,
                    origin,
                    camera,
                    nest,
                }
            },
        )
}

fn build(specs: &[Spec]) -> Document {
    let mut doc = Document::new("prop");
    let layer = *doc.layers.keys().next().unwrap();
    let mut last_group: Option<GroupId> = None;
    for s in specs {
        let group = if s.nest {
            let id = GroupId(doc.alloc_id());
            doc.groups.insert(
                id,
                Group {
                    flags: Default::default(),
                    id,
                    name: s.classname.clone(),
                    layer,
                    parent: last_group,
                    extra: Extra::new(),
                },
            );
            last_group = Some(id);
            Some(id)
        } else {
            last_group
        };

        let eid = EntityId(doc.alloc_id());
        let mut properties = Properties::new();
        for (k, v) in &s.props {
            properties.insert(
                k.clone(),
                match v {
                    PropValue::Bool(b) => PropertyValue::Bool(*b),
                    PropValue::Num(n) => PropertyValue::Number(*n),
                    PropValue::Vec(v) => PropertyValue::Vec3(DVec3::from(*v)),
                    PropValue::Str(s) => PropertyValue::String(s.clone()),
                },
            );
        }
        doc.entities.insert(
            eid,
            Entity {
                flags: Default::default(),
                id: eid,
                classname: s.classname.clone(),
                layer,
                group,
                origin: s.origin.map(DVec3::from),
                angles: DVec3::ZERO,
                properties,
                extra: Extra::new(),
            },
        );

        let bid = BrushId(doc.alloc_id());
        let lo = DVec3::new(s.min[0] as f64, s.min[1] as f64, s.min[2] as f64);
        let hi = lo + DVec3::new(s.size[0] as f64, s.size[1] as f64, s.size[2] as f64);
        let mut brush = cuboid(lo, hi);
        brush.faces[0].attrs = FaceAttrs {
            material: s.material.clone(),
            offset: DVec2::new(s.offset.0, s.offset.1),
            scale: DVec2::new(s.scale.0, s.scale.1),
            rotation: s.rotation,
        };
        doc.brushes.insert(
            bid,
            BrushObject {
                flags: Default::default(),
                id: bid,
                layer,
                group,
                entity: s.origin.is_none().then_some(eid),
                brush,
                extra: Extra::new(),
            },
        );

        if let Some((yaw, pitch, fov, ortho)) = s.camera {
            let cid = CameraId(doc.alloc_id());
            doc.cameras.insert(
                cid,
                Camera {
                    id: cid,
                    name: s.material.clone(),
                    position: lo,
                    yaw,
                    pitch,
                    projection: if ortho {
                        Projection::Orthographic {
                            view: OrthoView::Front,
                            zoom: fov,
                        }
                    } else {
                        Projection::Perspective { fov }
                    },
                    extra: Extra::new(),
                },
            );
        }
    }
    doc
}

proptest! {
    #[test]
    fn write_read_round_trip(specs in prop::collection::vec(spec(), 0..5)) {
        let doc = build(&specs);
        let text = write(&doc);
        let back = read(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        prop_assert_eq!(&back, &doc);
        prop_assert_eq!(write(&back), text);
    }
}
