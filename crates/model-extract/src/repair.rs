//! The fixed words of a repair prompt and of the prompted mode. A fault names a field and a kind,
//! never the model's output, which may hold untrusted text.

use model_provider::{ShapeFault, ShapeKind};

/// The user turn that asks for a corrected reply.
pub(crate) fn repair_text(fault: &ShapeFault) -> String {
    let what = match fault {
        ShapeFault::NotJson => "the reply was not a single valid JSON value".to_owned(),
        ShapeFault::NotRepresentable => "the reply has no form in the requested format".to_owned(),
        ShapeFault::Mismatch { at, want } => {
            format!(
                "the value at `{}` must be {}",
                at.as_str(),
                kind_words(*want)
            )
        }
        ShapeFault::Missing { field } => format!("the field `{}` is missing", field.as_str()),
        ShapeFault::Unknown { field } => {
            format!("`{}` holds a field that is not allowed", field.as_str())
        }
    };
    format!(
        "Your previous reply was not accepted: {what}. Reply again with only the corrected JSON."
    )
}

fn kind_words(kind: ShapeKind) -> &'static str {
    match kind {
        ShapeKind::Choice => "one of the allowed strings",
        ShapeKind::Integer => "an integer within the allowed range",
        ShapeKind::Text => "text within the length limit",
        ShapeKind::Date => "a date written YYYY-MM-DD",
        ShapeKind::DateTime => "a date and time in RFC 3339",
        ShapeKind::Record => "an object with exactly the listed fields",
        ShapeKind::List => "a list within the length limit",
        ShapeKind::Optional => "null or its value",
        ShapeKind::Tagged => "an object with a known tag and its content",
    }
}

/// The system text of the prompted mode.
pub(crate) fn prompted_text(schema: &str) -> String {
    format!(
        "Respond with ONLY a single JSON value that matches this JSON Schema, with no other text:\n{schema}"
    )
}

/// The description of the synthetic tool.
pub(crate) const FINAL_RESULT_DESCRIPTION: &str =
    "Submit the final result. Call this once, with the answer as the arguments.";
