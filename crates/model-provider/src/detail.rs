//! What a provider said when it refused: its HTTP status and a bounded, redacted excerpt of its
//! message, so the person and the logs keep the reason.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ServerStatus;

/// The most characters of a provider's message that survive.
const EXCERPT_CHARS: usize = 240;
/// What replaces a word that looks like a credential.
const REDACTED: &str = "[redacted]";
/// Prefixes of well-known credential formats.
const KEY_PREFIXES: [&str; 9] = [
    "sk-", "sk_", "pk-", "ghp_", "gho_", "xoxb", "xoxp", "eyj", "or-v1",
];
/// A word ending in one of these, then `:` or `=`, introduces a secret value.
const SECRET_NAMES: [&str; 8] = [
    "key",
    "token",
    "secret",
    "password",
    "passwd",
    "auth",
    "sig",
    "signature",
];

/// A provider's answer to a refused request: the HTTP status and what it said, cleaned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDetail {
    pub status: ServerStatus,
    /// At most 240 characters, one line, with anything credential-shaped replaced. Empty when the
    /// provider gave no message.
    pub message: String,
}

impl ProviderDetail {
    /// Keeps `status` and a redacted, bounded excerpt of `raw_message`.
    pub fn new(status: u16, raw_message: &str) -> Self {
        Self {
            status: ServerStatus(status),
            message: redact_excerpt(raw_message),
        }
    }
}

impl fmt::Display for ProviderDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.message.is_empty() {
            true => write!(f, "http {}", self.status.0),
            false => write!(f, "http {}: {}", self.status.0, self.message),
        }
    }
}

/// One line of `raw`, every credential-shaped word replaced, cut to a fixed length. Redaction runs
/// before the cut, so a token cut in half cannot slip through.
pub fn redact_excerpt(raw: &str) -> String {
    let mut words: Vec<&str> = Vec::new();
    let mut next_is_secret = false;
    for word in raw.split_whitespace() {
        let core = word.trim_matches(|c: char| {
            matches!(
                c,
                '"' | '\'' | '(' | ')' | ',' | ';' | '<' | '>' | '[' | ']' | '{' | '}' | '`'
            )
        });
        let lower = core.to_ascii_lowercase();
        let secret = next_is_secret || is_credential(core, &lower);
        next_is_secret = introduces_secret(&lower);
        words.push(if secret { REDACTED } else { word });
    }
    let line = words.join(" ");
    let mut chars = line.chars();
    let kept: String = chars.by_ref().take(EXCERPT_CHARS).collect();
    match chars.next() {
        Some(_) => format!("{kept}…"),
        None => kept,
    }
}

/// A word that is a known key format, a long opaque run, or `name=value` of a secret name.
fn is_credential(core: &str, lower: &str) -> bool {
    let length = core.chars().count();
    let key_alphabet = core
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '+' | '/' | '='));
    let has_digit = core.chars().any(|c| c.is_ascii_digit());
    let known_prefix = length >= 8 && KEY_PREFIXES.iter().any(|p| lower.starts_with(p));
    let opaque = key_alphabet && ((length >= 24 && has_digit) || length >= 40);
    known_prefix || opaque || has_secret_value(lower)
}

/// `api_key=abc` or `token:abc`: a secret name, a separator, then a value.
fn has_secret_value(lower: &str) -> bool {
    match lower.split_once(|c: char| c == '=' || c == ':') {
        Some((name, value)) => !value.is_empty() && names_a_secret(name),
        None => false,
    }
}

/// The word says the next word is a secret: `Bearer`, `Authorization:`, `api_key=`, `token:`.
fn introduces_secret(lower: &str) -> bool {
    lower == "bearer"
        || lower == "authorization:"
        || lower
            .strip_suffix(|c: char| c == '=' || c == ':')
            .is_some_and(names_a_secret)
}

fn names_a_secret(name: &str) -> bool {
    SECRET_NAMES.iter().any(|n| name.ends_with(n))
}
