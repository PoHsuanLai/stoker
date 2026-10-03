//! `choose`: the four rules, in order.

mod support;

use model_extract::{ExtractMode, FINAL_RESULT_TOOL, ToolsPresent, choose};
use model_provider::{
    CharCount, ChoiceText, Constraint, Count, Field, FieldName, OutputShape, SchemaDialect, Shape,
    ShapeWithTools, ToolName, ToolSupport,
};
use support::caps;

fn choice() -> Shape {
    Shape::Choice(vec![ChoiceText("allow".into()), ChoiceText("deny".into())])
}

fn record() -> Shape {
    Shape::Record(vec![Field {
        name: FieldName::new("note").unwrap(),
        shape: Shape::Text { max: CharCount(20) },
    }])
}

fn tool_call() -> ExtractMode {
    ExtractMode::ToolCall {
        tool: ToolName::new(FINAL_RESULT_TOOL).unwrap(),
    }
}

use Constraint::{Choice, Gbnf, JsonSchema, Lark, Regex};
use ShapeWithTools::{AfterResult, Refuse, Together};
use ToolSupport::{Absent, ServerParsed};
use ToolsPresent::{No, Yes};

#[test]
fn choose_picks_native_choice_where_the_engine_has_it() {
    let got = choose(
        &caps(&[Choice, Regex, Gbnf, JsonSchema], Absent),
        &choice(),
        No,
        Together,
    );
    assert_eq!(
        got,
        ExtractMode::Native(OutputShape::Choice(vec!["allow".into(), "deny".into()]))
    );
}

#[test]
fn choose_prefers_the_narrowest_constraint_for_a_scalar() {
    let shape = choice();
    let regex = choose(
        &caps(&[Regex, Gbnf, JsonSchema], Absent),
        &shape,
        No,
        Together,
    );
    assert_eq!(
        regex,
        ExtractMode::Native(OutputShape::Regex("allow|deny".into()))
    );
    let gbnf = choose(&caps(&[Gbnf, JsonSchema], Absent), &shape, No, Together);
    assert_eq!(
        gbnf,
        ExtractMode::Native(OutputShape::Gbnf(shape.to_gbnf().unwrap()))
    );
    let int = Shape::Integer { min: 0, max: 9 };
    let regex = choose(&caps(&[Regex, JsonSchema], Absent), &int, No, Together);
    assert_eq!(
        regex,
        ExtractMode::Native(OutputShape::Regex("[0-9]".into()))
    );
}

#[test]
fn choose_picks_the_schema_for_a_record_and_a_grammar_when_there_is_no_schema() {
    let shape = record();
    let schema = choose(&caps(&[JsonSchema, Gbnf], Absent), &shape, No, Together);
    assert_eq!(
        schema,
        ExtractMode::Native(OutputShape::JsonSchema(
            shape.to_json_schema(SchemaDialect::Plain)
        ))
    );
    let gbnf = choose(&caps(&[Gbnf], Absent), &shape, No, Together);
    assert_eq!(
        gbnf,
        ExtractMode::Native(OutputShape::Gbnf(shape.to_gbnf().unwrap()))
    );
}

#[test]
fn choose_falls_back_to_tool_when_schema_and_tools_conflict() {
    for with in [AfterResult, Refuse] {
        assert_eq!(
            choose(
                &caps(&[JsonSchema, Gbnf, Choice], ServerParsed),
                &record(),
                Yes,
                with
            ),
            tool_call(),
            "{with:?}"
        );
        assert_eq!(
            choose(&caps(&[Choice, Gbnf], ServerParsed), &choice(), Yes, with),
            tool_call(),
            "{with:?}"
        );
    }
    // An engine that composes both keeps the constraint.
    assert!(matches!(
        choose(&caps(&[JsonSchema], ServerParsed), &record(), Yes, Together),
        ExtractMode::Native(OutputShape::JsonSchema(_))
    ));
    // Tools in the request do not matter when the request has none of its own to protect.
    assert!(matches!(
        choose(&caps(&[JsonSchema], ServerParsed), &record(), No, Refuse),
        ExtractMode::Native(_)
    ));
}

#[test]
fn choose_uses_a_tool_when_nothing_constrains_and_the_prompt_last() {
    assert_eq!(
        choose(&caps(&[], ServerParsed), &record(), No, Together),
        tool_call()
    );
    assert_eq!(
        choose(&caps(&[], ToolSupport::Native), &choice(), No, Together),
        tool_call()
    );
    assert_eq!(
        choose(&caps(&[], Absent), &record(), No, Together),
        ExtractMode::Prompted
    );
    // Lark alone cannot say a shape here.
    assert_eq!(
        choose(&caps(&[Lark], Absent), &record(), No, Together),
        ExtractMode::Prompted
    );
}

#[test]
fn a_grammar_that_cannot_say_the_shape_is_skipped() {
    let dated = Shape::Record(vec![Field {
        name: FieldName::new("on").unwrap(),
        shape: Shape::Date,
    }]);
    assert_eq!(
        choose(&caps(&[Gbnf], Absent), &dated, No, Together),
        ExtractMode::Prompted
    );
    assert_eq!(
        choose(
            &caps(&[Gbnf, Choice], ToolSupport::Native),
            &Shape::List {
                of: Box::new(Shape::Date),
                max: Count(2)
            },
            No,
            Together
        ),
        tool_call()
    );
}
