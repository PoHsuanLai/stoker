//! A scanner for the call syntax of the text dialects: `verb(name='value', name=bare)`, one
//! call after another. It reads text and never evaluates it.

use crate::{ByteOffset, ParseError};

/// One call: the verb and its named arguments, values unescaped.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Call<'a> {
    pub verb: &'a str,
    pub args: Vec<(&'a str, String)>,
}

impl Call<'_> {
    pub(crate) fn arg(&self, name: &str) -> Option<&str> {
        self.args
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Every call in `src`, which sits at byte `base` of the original reply. Once one call has been
/// read, anything after the last call that is not a call (an end-of-turn token) is ignored; a
/// call that does not end is `Unterminated`, and text that is not a call where the first one
/// should be is `Malformed`.
pub(crate) fn calls(src: &str, base: usize) -> Result<Vec<Call<'_>>, ParseError> {
    let mut cur = Cursor { src, pos: 0, base };
    let mut out = Vec::new();
    loop {
        cur.skip_ws();
        if cur.peek().is_none() {
            return Ok(out);
        }
        match cur.call() {
            Ok(call) => out.push(call),
            Err(ParseError::Malformed { .. }) if !out.is_empty() => return Ok(out),
            Err(e) => return Err(e),
        }
    }
}

struct Cursor<'a> {
    src: &'a str,
    pos: usize,
    base: usize,
}

impl<'a> Cursor<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) {
        self.pos += self.peek().map_or(0, char::len_utf8);
    }

    fn skip_ws(&mut self) {
        self.pos += self.rest().len() - self.rest().trim_start().len();
    }

    fn malformed(&self) -> ParseError {
        let at = u32::try_from(self.base + self.pos).unwrap_or(u32::MAX);
        ParseError::Malformed { at: ByteOffset(at) }
    }

    /// The next character must be `want`; the end of the text means the call never ended.
    fn expect(&mut self, want: char) -> Result<(), ParseError> {
        match self.peek() {
            Some(c) if c == want => {
                self.bump();
                Ok(())
            }
            None => Err(ParseError::Unterminated),
            Some(_) => Err(self.malformed()),
        }
    }

    fn ident(&mut self) -> Result<&'a str, ParseError> {
        let starts_well = self
            .peek()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
        let len = self
            .rest()
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(self.rest().len());
        if len == 0 || !starts_well {
            return Err(if self.peek().is_none() {
                ParseError::Unterminated
            } else {
                self.malformed()
            });
        }
        let ident = &self.rest()[..len];
        self.pos += len;
        Ok(ident)
    }

    fn call(&mut self) -> Result<Call<'a>, ParseError> {
        let verb = self.ident()?;
        self.skip_ws();
        self.expect('(')?;
        let mut args = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() == Some(')') {
                self.bump();
                return Ok(Call { verb, args });
            }
            let name = self.ident()?;
            self.skip_ws();
            self.expect('=')?;
            self.skip_ws();
            args.push((name, self.value()?));
            self.skip_ws();
            match self.peek() {
                Some(',') => self.bump(),
                Some(')') => {}
                None => return Err(ParseError::Unterminated),
                Some(_) => return Err(self.malformed()),
            }
        }
    }

    fn value(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            Some(q @ ('\'' | '"')) => {
                self.bump();
                self.quoted(q)
            }
            Some(_) => self.bare(),
            None => Err(ParseError::Unterminated),
        }
    }

    /// Up to the next `,` or `)` outside brackets.
    fn bare(&mut self) -> Result<String, ParseError> {
        let mut depth = 0u32;
        let end = self.rest().char_indices().find(|&(_, c)| match c {
            '(' | '[' => {
                depth += 1;
                false
            }
            ')' | ']' if depth > 0 => {
                depth -= 1;
                false
            }
            ',' | ')' if depth == 0 => true,
            _ => false,
        });
        let len = end.map(|(i, _)| i).ok_or(ParseError::Unterminated)?;
        let text = self.rest()[..len].trim().to_owned();
        self.pos += len;
        Ok(text)
    }

    /// A quoted value. Models leave quotes inside `type` text unescaped, so a quote closes the
    /// value only when what follows is the end of the call or the next `name=`.
    fn quoted(&mut self, quote: char) -> Result<String, ParseError> {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            self.bump();
            match c {
                '\\' => out.push_str(&self.escape()),
                c if c == quote && closes(self.rest()) => return Ok(out),
                c => out.push(c),
            }
        }
        Err(ParseError::Unterminated)
    }

    fn escape(&mut self) -> String {
        let Some(c) = self.peek() else {
            return "\\".into();
        };
        self.bump();
        match c {
            'n' => "\n".into(),
            't' => "\t".into(),
            '\\' | '\'' | '"' => c.to_string(),
            other => format!("\\{other}"),
        }
    }
}

/// Whether the text after a quote ends the argument: `)` or `, name=`.
fn closes(rest: &str) -> bool {
    let rest = rest.trim_start();
    if rest.starts_with(')') {
        return true;
    }
    let Some(after) = rest.strip_prefix(',') else {
        return false;
    };
    let after = after.trim_start();
    let name_len = after
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(after.len());
    name_len > 0 && after[name_len..].trim_start().starts_with('=')
}
