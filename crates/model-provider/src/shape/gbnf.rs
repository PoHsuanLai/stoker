//! `Shape` to GBNF, the grammar format of llama-server (`grammar` on a request).
//!
//! The grammar describes the canonical text of a value: fields in declared order, every field
//! present (an `Optional` is `null` or its value), no escapes written for non-ASCII. It is a
//! subset of what `Shape::check` accepts, so a reply that follows it always passes the check.
//! Repetition bounds above `UNROLL_MAX` are left open in the grammar (llama.cpp unrolls `{m,n}`,
//! so a large bound makes a huge grammar) and the checker enforces them.

use crate::shape::pattern::{Syntax, gbnf_literal, int_range};
use crate::{Shape, ShapeFault};

const UNROLL_MAX: u32 = 256;

const PRELUDE: &str = r#"ws ::= [ \t\n]{0,4}
char ::= [^"\\\x00-\x1F] | "\\" (["\\/bfnrt] | "u" [0-9a-fA-F]{4})
"#;

impl Shape {
    pub(crate) fn gbnf_text(&self) -> Result<String, ShapeFault> {
        let mut rules = Rules::default();
        let root = rules.expr(self)?;
        let body: String = rules
            .defs
            .iter()
            .map(|(name, expr)| format!("{name} ::= {expr}\n"))
            .collect();
        Ok(format!("root ::= {root}\n{PRELUDE}{body}"))
    }
}

#[derive(Default)]
struct Rules {
    defs: Vec<(String, String)>,
}

impl Rules {
    /// The expression for `shape`, adding named rules for the parts that repeat or nest.
    fn expr(&mut self, shape: &Shape) -> Result<String, ShapeFault> {
        match shape {
            Shape::Choice(choices) if choices.is_empty() => Err(ShapeFault::NotRepresentable),
            Shape::Choice(choices) => Ok(choices
                .iter()
                .map(|c| gbnf_literal(&json_string(&c.0)))
                .collect::<Vec<_>>()
                .join(" | ")),
            Shape::Integer { min, max } => int_range(*min, *max)
                .map(|p| p.render(Syntax::Gbnf))
                .ok_or(ShapeFault::NotRepresentable),
            Shape::Text { max } => {
                let count = if max.0 <= UNROLL_MAX {
                    format!("{{0,{}}}", max.0)
                } else {
                    "*".to_owned()
                };
                Ok(format!("\"\\\"\" char{count} \"\\\"\""))
            }
            Shape::Optional(inner) => {
                let inner = self.named("opt", inner)?;
                Ok(format!("{inner} | \"null\""))
            }
            Shape::List { of, max } => {
                let item = self.named("item", of)?;
                let tail = match max.0 {
                    0 => return Ok("\"[\" ws \"]\"".to_owned()),
                    1 => String::new(),
                    n if n <= UNROLL_MAX => format!(" (\",\" ws {item}){{0,{}}}", n - 1),
                    _ => format!(" (\",\" ws {item})*"),
                };
                Ok(format!("\"[\" ws ({item}{tail})? ws \"]\""))
            }
            Shape::Record(fields) => {
                let members = fields
                    .iter()
                    .map(|f| {
                        let value = self.named("field", &f.shape)?;
                        let key = gbnf_literal(&json_string(f.name.as_str()));
                        Ok(format!("{key} ws \":\" ws {value}"))
                    })
                    .collect::<Result<Vec<_>, ShapeFault>>()?;
                Ok(match members.is_empty() {
                    true => "\"{\" ws \"}\"".to_owned(),
                    false => format!("\"{{\" ws {} ws \"}}\"", members.join(" ws \",\" ws ")),
                })
            }
            Shape::Date | Shape::DateTime | Shape::Tagged { .. } => {
                Err(ShapeFault::NotRepresentable)
            }
        }
    }

    /// A rule for `shape`, returned as a reference.
    fn named(&mut self, stem: &str, shape: &Shape) -> Result<String, ShapeFault> {
        let expr = self.expr(shape)?;
        let name = format!("{stem}-{}", self.defs.len());
        self.defs.push((name.clone(), expr));
        Ok(name)
    }
}

fn json_string(text: &str) -> String {
    serde_json::Value::String(text.to_owned()).to_string()
}
