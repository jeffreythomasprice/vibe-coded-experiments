use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    /// JSON pointer of the offending part of the input; empty for the whole value.
    pub pointer: String,
    /// JSON pointer into the schema of the keyword that failed.
    pub schema_pointer: String,
    pub message: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.pointer.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.pointer, self.message)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("not valid JSON: {message}")]
    Syntax { message: String },
    #[error("{}", join(.0))]
    Invalid(Vec<Violation>),
    /// The input satisfies the schema but does not fit the generated type: the schema and the code disagree.
    #[error("{pointer}: {message}")]
    Deserialize { pointer: String, message: String },
}

impl ParseError {
    pub fn violations(&self) -> &[Violation] {
        match self {
            ParseError::Invalid(v) => v,
            _ => &[],
        }
    }

    /// JSON pointer of the first problem, if the error has one.
    pub fn pointer(&self) -> Option<&str> {
        match self {
            ParseError::Invalid(v) => v.first().map(|v| v.pointer.as_str()),
            ParseError::Deserialize { pointer, .. } => Some(pointer),
            ParseError::Syntax { .. } => None,
        }
    }
}

fn join(violations: &[Violation]) -> String {
    violations
        .iter()
        .map(Violation::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BundleError {
    #[error("{file}: not valid JSON: {message}")]
    Syntax { file: String, message: String },
    #[error("definition `{name}` is in both {first} and {second}")]
    DuplicateDef {
        name: String,
        first: String,
        second: String,
    },
    #[error("no definition named `{name}`")]
    UnknownDef { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SchemaCompileError {
    #[error("schema is not valid: {0}")]
    Invalid(String),
}
