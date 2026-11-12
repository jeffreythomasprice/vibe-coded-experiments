use editor_core::{Document, Entity, EntityId, Extra, Properties};
use editor_sim::components::{Keyframe, KeyframeAnimation, Track};
use editor_sim::{LoadWarning, Sim};
use glam::DVec3;
use serde_json::json;

fn track(interpolation: &str, keys: serde_json::Value) -> Track {
    let anim: KeyframeAnimation = editor_schema::parse(&json!({
        "interpolation": interpolation,
        "keyframes": keys,
    }))
    .unwrap();
    Track::new(&anim)
}

fn close(a: DVec3, b: DVec3) {
    assert!((a - b).length() < 1e-9, "{a:?} != {b:?}");
}

#[test]
fn linear_sampling_wraps_and_holds() {
    let t = track(
        "linear",
        json!([{ "time": 1, "offset": [0, 0, 0] }, { "time": 3, "offset": [0, 0, 10] }]),
    );
    assert_eq!(t.duration(), 3.0);
    close(t.sample(0.0).offset, DVec3::ZERO);
    close(t.sample(1.0).offset, DVec3::ZERO);
    close(t.sample(2.0).offset, DVec3::new(0.0, 0.0, 5.0));
    close(t.sample(3.5).offset, DVec3::ZERO);
    close(t.sample(5.0).offset, DVec3::new(0.0, 0.0, 5.0));
    close(t.sample(-1.0).offset, DVec3::new(0.0, 0.0, 5.0));
}

#[test]
fn unsorted_keyframes_are_sorted() {
    let t = track(
        "linear",
        json!([{ "time": 2, "offset": [20, 0, 0] }, { "time": 0, "offset": [0, 0, 0] }]),
    );
    close(t.sample(1.0).offset, DVec3::new(10.0, 0.0, 0.0));
}

#[test]
fn smooth_eases_in_and_out() {
    let t = track(
        "smooth",
        json!([{ "time": 0 }, { "time": 4, "offset": [8, 0, 0] }]),
    );
    close(t.sample(1.0).offset, DVec3::new(8.0 * 0.15625, 0.0, 0.0));
    close(t.sample(2.0).offset, DVec3::new(4.0, 0.0, 0.0));
}

#[test]
fn angles_interpolate_past_180_degrees() {
    let t = track(
        "linear",
        json!([{ "time": 0 }, { "time": 2, "angles": [0, 360, 0] }]),
    );
    close(t.sample(0.5).angles, DVec3::new(0.0, 90.0, 0.0));
    close(t.sample(1.5).angles, DVec3::new(0.0, 270.0, 0.0));
}

#[test]
fn single_keyframe_is_static() {
    let t = track("linear", json!([{ "time": 0, "offset": [1, 2, 3] }]));
    close(t.sample(10.0).offset, DVec3::new(1.0, 2.0, 3.0));
    let zero = track(
        "linear",
        json!([{ "time": 0 }, { "time": 0, "offset": [1, 2, 3] }]),
    );
    close(zero.sample(4.0).offset, DVec3::ZERO);
}

#[test]
fn keyframe_type_has_defaults() {
    let anim: KeyframeAnimation =
        editor_schema::parse(&json!({ "keyframes": [{ "time": 1 }] })).unwrap();
    let Keyframe { offset, angles, .. } = &anim.keyframes[0];
    assert_eq!((*offset, *angles), ([0.0; 3], [0.0; 3]));
}

fn demo() -> (String, Document) {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scenes/demo-courtyard.scene.toml"
    );
    let text = std::fs::read_to_string(path).unwrap();
    let doc = editor_fmt::read(&text).unwrap();
    (text, doc)
}

#[test]
fn demo_scene_is_canonical_and_loads_without_warnings() {
    let (text, doc) = demo();
    assert_eq!(editor_fmt::write(&doc), text);
    let (sim, warnings) = Sim::load(&doc);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(!sim.static_geometry().is_empty());
    let spawn = sim.spawn().expect("player start");
    close(spawn.translation, DVec3::new(0.0, -260.0, 32.0));
    assert_eq!(sim.poses().len(), 2);
}

#[test]
fn door_leaf_swings_about_its_hinge() {
    let (_, doc) = demo();
    let (mut sim, _) = Sim::load(&doc);
    let leaf = EntityId(33);
    let tip = DVec3::new(-8.0, 56.0, 0.0);

    sim.set_time(0.0);
    close(sim.poses()[&leaf].transform_point3(tip), tip);

    sim.set_time(1.5);
    close(
        sim.poses()[&leaf].transform_point3(tip),
        DVec3::new(-64.0, 112.0, 0.0),
    );
    close(
        sim.poses()[&EntityId(61)].transform_point3(DVec3::new(8.0, 56.0, 0.0)),
        DVec3::new(64.0, 112.0, 0.0),
    );

    sim.set_time(7.0);
    close(sim.poses()[&leaf].transform_point3(tip), tip);
    sim.set_time(0.0);
    sim.step(1.5);
    assert_eq!(sim.time(), 1.5);
}

#[test]
fn bad_components_are_skipped_with_a_warning() {
    let mut doc = Document::new("t");
    let layer = *doc.layers.keys().next().unwrap();
    let id = EntityId(doc.alloc_id());
    let mut bad = Properties::new();
    bad.insert("keyframes".into(), json!([]));
    doc.entities.insert(
        id,
        Entity {
            flags: Default::default(),
            id,
            layer,
            group: None,
            origin: DVec3::ZERO,
            angles: DVec3::ZERO,
            components: [
                ("keyframe_animation".to_string(), bad),
                ("custom".to_string(), Properties::new()),
            ]
            .into(),
            extra: Extra::new(),
        },
    );
    let (sim, warnings) = Sim::load(&doc);
    assert!(matches!(
        warnings.as_slice(),
        [LoadWarning::BadComponent { component, .. }] if component == "keyframe_animation"
    ));
    assert!(sim.poses().is_empty());
    let e = sim.entity(id).unwrap();
    assert!(
        sim.world()
            .get::<editor_sim::components::Dynamic>(e)
            .is_some()
    );
}

#[test]
fn omitted_layers_are_excluded() {
    let (_, mut doc) = demo();
    let props = *doc
        .layers
        .keys()
        .find(|l| doc.layers[l].name == "props")
        .unwrap();
    doc.layers.get_mut(&props).unwrap().omit_from_export = true;
    let (sim, _) = Sim::load(&doc);
    assert!(!sim.excluded().is_empty());
}

#[test]
fn world_reuses_slots_and_invalidates_handles() {
    let mut w = editor_sim::World::new();
    let a = w.spawn();
    w.insert(a, 5u32);
    assert_eq!(w.get::<u32>(a), Some(&5));
    assert!(w.despawn(a));
    let b = w.spawn();
    assert_eq!(a.index, b.index);
    assert!(w.get::<u32>(a).is_none());
    assert!(w.get::<u32>(b).is_none());
    assert!(!w.insert(a, 1u32));
}
