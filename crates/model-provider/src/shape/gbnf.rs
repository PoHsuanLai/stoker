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

/// The calendar a `Date` and a `DateTime` share: the days of each month (29 February is left
/// out, see `to_gbnf`), the clock and the zone. What the grammar reads, `check` reads too.
const CALENDAR: &[(&str, &str)] = &[
    ("day28", r#""0" [1-9] | "1" [0-9] | "2" [0-8]"#),
    ("day30", r#"day28 | "29" | "30""#),
    ("day31", r#"day30 | "31""#),
    (
        "month-day",
        r#"("01" | "03" | "05" | "07" | "08" | "10" | "12") "-" day31 | ("04" | "06" | "09" | "11") "-" day30 | "02-" day28"#,
    ),
    ("date-body", r#"[0-9]{4} "-" month-day"#),
    ("hour", r#"[01] [0-9] | "2" [0-3]"#),
    ("minute", r#"[0-5] [0-9]"#),
];
const DATE: &str = r#""\"" date-body "\"""#;
const DATE_TIME: &str = r#""\"" date-body "T" hour ":" minute ":" minute ("." [0-9]+)? ("Z" | ("+" | "-") hour ":" minute) "\"""#;

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
            Shape::Date => Ok(self.calendar("date", DATE)),
            Shape::DateTime => Ok(self.calendar("date-time", DATE_TIME)),
            Shape::Tagged { variants, .. } if variants.is_empty() => {
                Err(ShapeFault::NotRepresentable)
            }
            Shape::Tagged {
                tag,
                content,
                variants,
            } => {
                let key = |name: &str| gbnf_literal(&json_string(name));
                let arms = variants
                    .iter()
                    .map(|v| {
                        let value = self.named("content", &v.shape)?;
                        Ok(format!(
                            "\"{{\" ws {} ws \":\" ws {} ws \",\" ws {} ws \":\" ws {value} ws \"}}\"",
                            key(tag.as_str()),
                            key(v.name.as_str()),
                            key(content.as_str()),
                        ))
                    })
                    .collect::<Result<Vec<_>, ShapeFault>>()?;
                Ok(arms.join(" | "))
            }
            Shape::OrHandle(inner) => {
                let inner = self.named("alt", inner)?;
                let index = int_range(0, i64::MAX)
                    .map(|p| p.render(Syntax::Gbnf))
                    .ok_or(ShapeFault::NotRepresentable)?;
                let index = self.fixed("handle-index", &index);
                let body = format!("\"{{\" ws \"\\\"handle\\\"\" ws \":\" ws {index} ws \"}}\"");
                let handle = self.fixed("handle", &body);
                Ok(format!("{inner} | {handle}"))
            }
        }
    }

    /// The shared calendar rules, then `name` with its body; each is written once.
    fn calendar(&mut self, name: &str, body: &str) -> String {
        for (rule, text) in CALENDAR {
            self.fixed(rule, text);
        }
        self.fixed(name, body)
    }

    /// A rule with a fixed name and text, added the first time and returned as a reference.
    fn fixed(&mut self, name: &str, text: &str) -> String {
        if !self.defs.iter().any(|(n, _)| n == name) {
            self.defs.push((name.to_owned(), text.to_owned()));
        }
        name.to_owned()
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
