use std::path::PathBuf;

use serde_json::{Map, Value};
use typify::{TypeSpace, TypeSpaceSettings};

pub struct Config<'a> {
    /// Definitions to generate, with everything they reference.
    pub roots: &'a [&'a str],
    /// Definitions that already exist as Rust types: `(definition name, type path)`. They are referenced, not generated.
    pub replacements: &'a [(&'a str, &'a str)],
    /// Written to `$OUT_DIR/<output>.rs`.
    pub output: &'a str,
}

#[derive(Debug, thiserror::Error)]
pub enum GenError {
    #[error(transparent)]
    Bundle(#[from] editor_schema::BundleError),
    #[error("OUT_DIR is not set; call this from a build script")]
    NoOutDir,
    #[error("typify rejected the schemas: {0}")]
    Typify(String),
    #[error("generated code does not parse: {0}")]
    Syntax(#[from] syn::Error),
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
}

pub fn generate(config: &Config<'_>) -> Result<(), GenError> {
    println!("cargo:rerun-if-changed=../../schemas");
    let bundled = editor_schema::bundled();
    let defs = bundled["$defs"]
        .as_object()
        .expect("bundle always has $defs");
    let skip: Vec<&str> = config.replacements.iter().map(|(n, _)| *n).collect();
    let names = editor_schema::closure(defs, config.roots, &skip)?;

    let mut subset = Map::new();
    subset.insert("$schema".into(), bundled["$schema"].clone());
    subset.insert(
        "$defs".into(),
        Value::Object(names.iter().map(|n| (n.clone(), defs[n].clone())).collect()),
    );
    let root: schemars::schema::RootSchema = serde_json::from_value(Value::Object(subset))
        .map_err(|e| GenError::Typify(e.to_string()))?;

    let mut settings = TypeSpaceSettings::default();
    settings.with_derive("PartialEq".into());
    for (name, path) in config.replacements {
        settings.with_replacement(*name, *path, std::iter::empty());
    }
    let mut space = TypeSpace::new(&settings);
    space
        .add_root_schema(root)
        .map_err(|e| GenError::Typify(e.to_string()))?;

    let mut tokens = space.to_stream();
    for root in config.roots {
        let ident = syn::Ident::new(root, proc_macro2_span());
        tokens.extend(quote_impl(&ident, root));
    }
    let file = syn::parse2::<syn::File>(tokens)?;
    let out = PathBuf::from(std::env::var_os("OUT_DIR").ok_or(GenError::NoOutDir)?)
        .join(format!("{}.rs", config.output));
    std::fs::write(&out, prettyplease::unparse(&file)).map_err(|e| GenError::Io(out, e))
}

fn proc_macro2_span() -> proc_macro2::Span {
    proc_macro2::Span::call_site()
}

fn quote_impl(ident: &syn::Ident, name: &str) -> proc_macro2::TokenStream {
    let text = format!(
        "impl editor_schema::Schema for {ident} {{ const NAME: &'static str = {name:?}; }}"
    );
    text.parse().expect("impl tokens")
}
