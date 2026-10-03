//! A tiny pattern tree that renders as a regex or as GBNF, so an integer range and a choice are
//! spelled once for both languages.

/// Which language a pattern is rendered in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Syntax {
    Regex,
    Gbnf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Pat {
    /// Exact text.
    Lit(String),
    /// One digit from `lo` to `hi`, inclusive.
    Digits(u8, u8),
    Seq(Vec<Pat>),
    Alt(Vec<Pat>),
    /// `pat` repeated `min` to `max` times.
    Rep {
        pat: Box<Pat>,
        min: u32,
        max: u32,
    },
}

impl Pat {
    pub(crate) fn render(&self, syntax: Syntax) -> String {
        match self {
            Pat::Lit(text) => match syntax {
                Syntax::Regex => escape_regex(text),
                Syntax::Gbnf => gbnf_literal(text),
            },
            Pat::Digits(lo, hi) if lo == hi => match syntax {
                Syntax::Regex => lo.to_string(),
                Syntax::Gbnf => format!("\"{lo}\""),
            },
            Pat::Digits(lo, hi) => format!("[{lo}-{hi}]"),
            Pat::Seq(items) => items
                .iter()
                .map(|item| item.render_in_seq(syntax))
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(match syntax {
                    Syntax::Regex => "",
                    Syntax::Gbnf => " ",
                }),
            Pat::Alt(items) => items
                .iter()
                .map(|item| item.render(syntax))
                .collect::<Vec<_>>()
                .join(match syntax {
                    Syntax::Regex => "|",
                    Syntax::Gbnf => " | ",
                }),
            Pat::Rep { pat, min, max } => {
                format!("{}{{{min},{max}}}", pat.render_atom(syntax))
            }
        }
    }

    fn render_in_seq(&self, syntax: Syntax) -> String {
        match self {
            Pat::Alt(items) if items.len() > 1 => self.group(syntax),
            _ => self.render(syntax),
        }
    }

    fn render_atom(&self, syntax: Syntax) -> String {
        match self {
            Pat::Digits(..) => self.render(syntax),
            Pat::Lit(text) if text.chars().count() == 1 && syntax == Syntax::Regex => {
                self.render(syntax)
            }
            _ => self.group(syntax),
        }
    }

    fn group(&self, syntax: Syntax) -> String {
        let open = match syntax {
            Syntax::Regex => "(?:",
            Syntax::Gbnf => "(",
        };
        format!("{open}{})", self.render(syntax))
    }
}

pub(crate) fn escape_regex(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            let special = "\\.^$*+?()[]{}|/-".contains(c);
            special.then_some('\\').into_iter().chain([c])
        })
        .collect()
}

/// A GBNF string literal: double quoted, with `"`, `\` and control characters escaped.
pub(crate) fn gbnf_literal(text: &str) -> String {
    let body: String = text
        .chars()
        .flat_map(|c| match c {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            c if c.is_control() => format!("\\u{:04X}", u32::from(c)).chars().collect(),
            c => vec![c],
        })
        .collect();
    format!("\"{body}\"")
}

/// The decimal integers from `min` to `max`, written without leading zeros or a plus sign.
/// `None` when `min > max`.
pub(crate) fn int_range(min: i64, max: i64) -> Option<Pat> {
    if min > max {
        return None;
    }
    let (min, max) = (i128::from(min), i128::from(max));
    let negative = (min < 0).then(|| {
        let top = -min;
        let bottom = (-max).max(1);
        Pat::Seq(vec![Pat::Lit("-".into()), unsigned(bottom, top)])
    });
    let nonnegative = (max >= 0).then(|| unsigned(min.max(0), max));
    let all: Vec<Pat> = negative.into_iter().chain(nonnegative).collect();
    Some(Pat::Alt(all))
}

/// The unsigned integers in `lo..=hi` (`lo <= hi`), split by digit count.
fn unsigned(lo: i128, hi: i128) -> Pat {
    let lengths = digits(lo).len()..=digits(hi).len();
    let parts = lengths
        .map(|len| {
            let floor = if len == 1 {
                0
            } else {
                10_i128.pow(len as u32 - 1)
            };
            let ceil = 10_i128.pow(len as u32) - 1;
            (lo.max(floor), hi.min(ceil))
        })
        .map(|(a, b)| same_length(&digits(a), &digits(b)))
        .collect();
    Pat::Alt(parts)
}

fn digits(n: i128) -> Vec<u8> {
    n.to_string().bytes().map(|b| b - b'0').collect()
}

/// Numbers with the digit strings `lo..=hi` of one length.
fn same_length(lo: &[u8], hi: &[u8]) -> Pat {
    let (Some((&l0, lrest)), Some((&h0, hrest))) = (lo.split_first(), hi.split_first()) else {
        return Pat::Seq(Vec::new());
    };
    let digit = |d: u8| Pat::Digits(d, d);
    let free = |n: usize| match n {
        1 => Pat::Digits(0, 9),
        n => Pat::Rep {
            pat: Box::new(Pat::Digits(0, 9)),
            min: n as u32,
            max: n as u32,
        },
    };
    if lrest.is_empty() {
        return Pat::Digits(l0, h0);
    }
    if l0 == h0 {
        return Pat::Seq(vec![digit(l0), same_length(lrest, hrest)]);
    }
    let low_full = lrest.iter().all(|d| *d == 0);
    let high_full = hrest.iter().all(|d| *d == 9);
    let first = if low_full { l0 } else { l0 + 1 };
    let last = if high_full { h0 } else { h0 - 1 };
    let mut alts = Vec::new();
    if !low_full {
        alts.push(Pat::Seq(vec![
            digit(l0),
            same_length(lrest, &vec![9; lrest.len()]),
        ]));
    }
    if first <= last {
        alts.push(Pat::Seq(vec![Pat::Digits(first, last), free(lrest.len())]));
    }
    if !high_full {
        alts.push(Pat::Seq(vec![
            digit(h0),
            same_length(&vec![0; hrest.len()], hrest),
        ]));
    }
    Pat::Alt(alts)
}
