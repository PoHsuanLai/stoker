//! The closed vocabulary of structured outputs, and the conversions from it.
//!
//! Decision: no `schemars` for runtime-typed outputs at first. Our output types are few and closed
//! (the reader's `ValueSchema`, the review verdict, the policy patch, consolidation facts; planner
//! steps are tool calls whose schemas already come from action declarations), we need regex,
//! choice and GBNF output that `schemars` cannot give, and our types are `Eq` newtypes whose serde
//! form is the schema. One conversion lives here so docket, almanac and cua do not each write one.
//! The checker is ours (`Shape::check`), not a `jsonschema` crate.
//!
mod check;
mod from_schema;
mod gbnf;
mod pattern;
mod schema;

pub use from_schema::{KeywordText, Refused, SchemaLimits, SchemaPath, SchemaRefusal};
pub use schema::sanitize_schema;

use serde::{Deserialize, Serialize};

use crate::{CharCount, Count, JsonText, SchemaText};

macro_rules! ident {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(name: impl Into<String>) -> Result<Self, IdentError> {
                let name = name.into();
                let mut chars = name.chars();
                let ok = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
                    && name.len() <= 64
                    && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
                if ok { Ok(Self(name)) } else { Err(IdentError) }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentError;
            fn try_from(name: String) -> Result<Self, IdentError> {
                $name::new(name)
            }
        }

        impl From<$name> for String {
            fn from(name: $name) -> String {
                name.0
            }
        }
    };
}

/// A name is not `[A-Za-z_][A-Za-z0-9_]{0,63}`. A field and a variant name end up inside a
/// schema and a grammar, so they are checked where they enter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a field or variant name is 1 to 64 characters of [A-Za-z0-9_], not starting with a digit")]
pub struct IdentError;

ident!(
    /// The name of a record field.
    FieldName
);
ident!(
    /// The name of a variant of a tagged enum.
    VariantName
);

/// One of the strings a `Shape::Choice` allows. Escaped by the converters, so any text is fine.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChoiceText(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: FieldName,
    pub shape: Shape,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variant {
    pub name: VariantName,
    pub shape: Shape,
}

/// What a structured reply looks like. The serde form of this enum is the file and wire form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Shape {
    Choice(Vec<ChoiceText>),
    Integer {
        min: i64,
        max: i64,
    },
    Text {
        max: CharCount,
    },
    Date,
    DateTime,
    Record(Vec<Field>),
    List {
        of: Box<Shape>,
        max: Count,
    },
    Optional(Box<Shape>),
    /// Our serde tag/content enums: `{ "<tag>": "<variant>", "<content>": <that variant's shape> }`.
    Tagged {
        tag: FieldName,
        content: FieldName,
        variants: Vec<Variant>,
    },
    /// The inner shape, or a handle: `{ "handle": n }` with `n` a non-negative integer. A planner
    /// that holds a value only by handle (a file, an entity it was shown) names it that way, and
    /// the router resolves it. An entity is a `Record` of its own fields, so "an entity or a
    /// handle" is `OrHandle(Record(..))`; a list of them is `List { of: OrHandle(..), .. }`.
    OrHandle(Box<Shape>),
}

/// The kind of a shape, for a fault that names where and what without echoing a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Choice,
    Integer,
    Text,
    Date,
    DateTime,
    Record,
    List,
    Optional,
    Tagged,
}

/// Which JSON Schema a provider accepts. Port of rig's `providers/internal/schema.rs` options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaDialect {
    Plain,
    /// `additionalProperties: false`, every property required, `$ref` siblings stripped,
    /// `oneOf` as `anyOf`.
    OpenAiStrict,
    /// Numeric bounds stripped.
    Anthropic,
}

/// Why a shape cannot be converted, or a value does not fit it. Names a field and a kind, never a
/// value: a model's text may be untrusted and must not travel into a repair prompt.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ShapeFault {
    #[error("this shape has no form in the requested language")]
    NotRepresentable,
    #[error("the reply is not JSON")]
    NotJson,
    #[error("field {at:?} is not a {want:?}")]
    Mismatch { at: FieldName, want: ShapeKind },
    #[error("field {field:?} is missing")]
    Missing { field: FieldName },
    #[error("field {field:?} is not allowed here")]
    Unknown { field: FieldName },
}

impl Shape {
    /// What a handle is: `{ "handle": n }`, `n` from 0.
    pub(crate) fn handle() -> Shape {
        Shape::Record(vec![Field {
            name: FieldName::new("handle").unwrap_or_else(|_| unreachable!("a valid name")),
            shape: Shape::Integer {
                min: 0,
                max: i64::MAX,
            },
        }])
    }

    pub fn to_json_schema(&self, dialect: SchemaDialect) -> SchemaText {
        self.schema_text(dialect)
    }

    /// GBNF for llama-server. Every shape has a form except an empty `Choice`, an empty `Integer`
    /// range and a `Tagged` with no variants. The grammar is a subset of what `check` accepts: a
    /// date reads every day a month has, except 29 February (a leap year is not a rule a grammar
    /// states), and every `Tagged` value writes its tag first.
    pub fn to_gbnf(&self) -> Result<String, ShapeFault> {
        self.gbnf_text()
    }

    /// A regex over the bare value, for Choice and a bounded Integer only: the reply is `allow`,
    /// not `"allow"` (vLLM's `structured_outputs.regex` constrains the whole reply).
    pub fn to_regex(&self) -> Option<String> {
        match self {
            Shape::Choice(choices) if !choices.is_empty() => Some(
                pattern::Pat::Alt(
                    choices
                        .iter()
                        .map(|c| pattern::Pat::Lit(c.0.clone()))
                        .collect(),
                )
                .render(pattern::Syntax::Regex),
            ),
            Shape::Integer { min, max } => {
                pattern::int_range(*min, *max).map(|p| p.render(pattern::Syntax::Regex))
            }
            _ => None,
        }
    }

    /// Our own checker for the same vocabulary.
    pub fn check(&self, json: &JsonText) -> Result<(), ShapeFault> {
        self.check_json(json)
    }
}

/// A typed output, implemented by hand for the few output types there are (checked at
/// construction and on deserialisation, so `read` is serde plus the checked constructors).
pub trait Extract: Sized {
    fn shape() -> Shape;
    fn read(json: &JsonText) -> Result<Self, ShapeFault>;
}
