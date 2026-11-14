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
            open: false,
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

fn set_open(doc: &mut Document, id: GroupId, open: bool) {
    let mut group = doc.groups.get(&id).unwrap().clone();
    group.open = open;
    doc.insert(Object::Group(group));
}

#[test]
fn clicks_select_the_outermost_closed_group() {
    let (mut doc, _, outer, inner, entity, brush) = setup();
    let b: ObjectId = brush.into();
    assert_eq!(doc.pick_target(b), ObjectId::from(outer));
    set_open(&mut doc, outer, true);
    assert_eq!(doc.pick_target(b), ObjectId::from(inner));
    assert_eq!(doc.pick_target(entity.into()), ObjectId::from(inner));
    set_open(&mut doc, inner, true);
    assert_eq!(doc.open_group(), Some(inner));
    assert_eq!(doc.pick_target(b), b);
    let lone = ObjectId::from(outer);
    set_open(&mut doc, outer, false);
    set_open(&mut doc, inner, false);
    assert_eq!(doc.pick_target(lone), lone);
}

#[test]
fn an_open_group_locks_everything_else() {
    let (mut doc, layer, outer, inner, _, brush) = setup();
    let b: ObjectId = brush.into();
    assert_eq!(doc.open_group(), None);
    assert!(doc.pickable(b));
    set_open(&mut doc, inner, true);
    assert_eq!(doc.open_group(), Some(inner));
    assert!(doc.pickable(b));
    assert!(!doc.effective_locked(outer.into()) && !doc.effective_locked(layer.into()));

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
    assert!(doc.effective_locked(other.into()));
    assert!(!doc.pickable(other.into()));
    assert!(doc.locked_ids().contains(&other.into()));
    set_open(&mut doc, inner, false);
    assert!(doc.pickable(other.into()));
    set_open(&mut doc, outer, true);
    assert!(doc.pickable(b));

    set(
        &mut doc,
        outer.into(),
        Flags {
            hidden: false,
            locked: true,
        },
    );
    assert!(!doc.pickable(b));
}
