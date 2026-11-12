use editor_core::*;

fn setup() -> (Document, LayerId, GroupId, GroupId, EntityId, BrushId) {
    let mut doc = Document::new("t");
    let layer = *doc.layers.keys().next().unwrap();
    let outer = GroupId(doc.alloc_id());
    let inner = GroupId(doc.alloc_id());
    let group = |id, parent| {
        Object::Group(Group {
            flags: Flags::default(),
            id,
            name: "g".into(),
            layer,
            parent,
            extra: Extra::new(),
        })
    };
    doc.insert(group(outer, None));
    doc.insert(group(inner, Some(outer)));
    let entity = EntityId(doc.alloc_id());
    doc.insert(Object::Entity(Entity {
        flags: Flags::default(),
        id: entity,
        layer,
        group: Some(inner),
        origin: DVec3::ZERO,
        angles: DVec3::ZERO,
        components: Default::default(),
        extra: Extra::new(),
    }));
    let brush = BrushId(doc.alloc_id());
    doc.insert(Object::Brush(BrushObject {
        flags: Flags::default(),
        id: brush,
        layer,
        group: Some(inner),
        entity: Some(entity),
        brush: Brush::default(),
        extra: Extra::new(),
    }));
    (doc, layer, outer, inner, entity, brush)
}

fn set(doc: &mut Document, id: ObjectId, flags: Flags) {
    let object = doc.get(id).unwrap().with_flags(flags);
    doc.insert(object);
}

#[test]
fn flags_are_inherited_from_every_container() {
    let (mut doc, layer, outer, _, entity, brush) = setup();
    let b: ObjectId = brush.into();
    assert_eq!(doc.effective_flags(b), Flags::default());

    for container in [ObjectId::from(layer), outer.into(), entity.into()] {
        let hidden = Flags {
            hidden: true,
            locked: false,
        };
        set(&mut doc, container, hidden);
        assert!(doc.effective_hidden(b), "{container}");
        assert!(!doc.effective_locked(b));
        set(&mut doc, container, Flags::default());
        assert!(!doc.effective_hidden(b));
    }

    set(
        &mut doc,
        outer.into(),
        Flags {
            hidden: false,
            locked: true,
        },
    );
    assert!(doc.effective_locked(b));
    assert!(doc.effective_locked(entity.into()));
    assert!(!doc.effective_locked(layer.into()));
}

#[test]
fn ancestors_run_entity_then_groups_then_layer() {
    let (doc, layer, outer, inner, entity, brush) = setup();
    assert_eq!(
        doc.ancestors(brush.into()),
        vec![entity.into(), inner.into(), outer.into(), layer.into()]
    );
    assert!(doc.ancestors(layer.into()).is_empty());
}

#[test]
fn clicks_select_the_outermost_closed_group() {
    let (doc, _, outer, inner, entity, brush) = setup();
    let b: ObjectId = brush.into();
    assert_eq!(doc.pick_target(b, None), ObjectId::from(outer));
    assert_eq!(doc.pick_target(b, Some(outer)), ObjectId::from(inner));
    assert_eq!(doc.pick_target(b, Some(inner)), b);
    assert_eq!(
        doc.pick_target(entity.into(), Some(outer)),
        ObjectId::from(inner)
    );
    let lone = ObjectId::from(outer);
    assert_eq!(doc.pick_target(lone, None), lone);
}

#[test]
fn an_open_group_leaves_everything_else_unpickable() {
    let (mut doc, layer, outer, inner, _, brush) = setup();
    let b: ObjectId = brush.into();
    assert!(doc.pickable(b, None));
    assert!(doc.pickable(b, Some(inner)));
    assert!(doc.pickable(b, Some(outer)));

    let other = BrushId(doc.alloc_id());
    doc.insert(Object::Brush(BrushObject {
        flags: Flags::default(),
        id: other,
        layer,
        group: None,
        entity: None,
        brush: Brush::default(),
        extra: Extra::new(),
    }));
    assert!(doc.pickable(other.into(), None));
    assert!(!doc.pickable(other.into(), Some(outer)));

    set(
        &mut doc,
        outer.into(),
        Flags {
            hidden: false,
            locked: true,
        },
    );
    assert!(!doc.pickable(b, None));
}
