//! Temporary bridge: types still deriving `schemars::JsonSchema` embed generated types, so those expose the committed schema through schemars. Removed once the embedding types are generated too.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator};

use crate::{ComponentDef, ComponentDefsFile, ComponentSize};
fn register(generator: &mut SchemaGenerator, name: &str) -> Schema {
    let defs = editor_schema::bundled()["$defs"]
        .as_object()
        .expect("bundle has $defs");
    let names = editor_schema::closure(defs, &[name], &[]).expect("committed definition");
    for n in names.into_iter().filter(|n| n != name) {
        generator
            .definitions_mut()
            .insert(n.clone(), defs[&n].clone());
    }
    Schema::try_from(defs[name].clone()).expect("a definition is a schema")
}

macro_rules! bridge {
    ($($ty:ty => $name:literal),* $(,)?) => {$(
        impl JsonSchema for $ty {
            fn schema_name() -> Cow<'static, str> {
                $name.into()
            }

            fn json_schema(generator: &mut SchemaGenerator) -> Schema {
                register(generator, $name)
            }
        }
    )*};
}

bridge!(
    ComponentDefsFile => "ComponentDefsFile",
    ComponentDef => "ComponentDef",
    ComponentSize => "ComponentSize",
);
