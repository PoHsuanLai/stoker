//! Checked names and the personal text: languages, voices, text to speak, text heard.

use core::fmt;

use serde::{Deserialize, Serialize};

/// The text is not 1 to 35 bytes of ASCII letters, digits and `-`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a language tag is 1 to 35 characters of letters, digits and '-'")]
pub struct LangError;

/// A BCP 47 language tag (`en`, `zh-TW`), checked for its character set only: the same grammar
/// as porter's `LanguageTag`, which inferd's bridge maps one to one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Lang(String);

impl Lang {
    pub fn new(text: impl Into<String>) -> Result<Self, LangError> {
        let text = text.into();
        let ok = !text.is_empty()
            && text.len() <= 35
            && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        if ok { Ok(Self(text)) } else { Err(LangError) }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Lang {
    type Error = LangError;
    fn try_from(text: String) -> Result<Self, LangError> {
        Lang::new(text)
    }
}

impl From<Lang> for String {
    fn from(lang: Lang) -> String {
        lang.0
    }
}

/// Which language to listen for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum LangChoice {
    /// The engine detects it.
    Auto,
    /// The first is the primary language.
    Prefer(Vec<Lang>),
}

/// The text is not 1 to 64 characters of `[A-Za-z0-9_.-]`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a voice id is 1 to 64 characters of [A-Za-z0-9_.-]")]
pub struct VoiceIdError;

/// A synthesiser's voice (`af_heart`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct VoiceId(String);

impl VoiceId {
    pub fn new(id: impl Into<String>) -> Result<Self, VoiceIdError> {
        let id = id.into();
        let ok = (1..=64).contains(&id.len())
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
        if ok { Ok(Self(id)) } else { Err(VoiceIdError) }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for VoiceId {
    type Error = VoiceIdError;
    fn try_from(id: String) -> Result<Self, VoiceIdError> {
        VoiceId::new(id)
    }
}

impl From<VoiceId> for String {
    fn from(id: VoiceId) -> String {
        id.0
    }
}

/// The text to speak is empty or longer than 4096 bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the text to speak is 1 to 4096 bytes")]
pub struct SpokenTextError;

/// Text to speak, 1 to 4096 bytes per request. It is what a model said, so `Debug` shows the
/// length only.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SpokenText(String);

impl SpokenText {
    pub const MAX_BYTES: usize = 4096;

    pub fn new(text: impl Into<String>) -> Result<Self, SpokenTextError> {
        let text = text.into();
        if (1..=Self::MAX_BYTES).contains(&text.len()) {
            Ok(Self(text))
        } else {
            Err(SpokenTextError)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SpokenText {
    type Error = SpokenTextError;
    fn try_from(text: String) -> Result<Self, SpokenTextError> {
        SpokenText::new(text)
    }
}

impl From<SpokenText> for String {
    fn from(text: SpokenText) -> String {
        text.0
    }
}

impl fmt::Debug for SpokenText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SpokenText(<{} bytes>)", self.0.len())
    }
}

/// Text an engine recognised: what the person said, so `Debug` shows the length only. It is
/// untrusted like any model output; it carries no authority.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HeardText(pub String);

impl fmt::Debug for HeardText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HeardText(<{} bytes>)", self.0.len())
    }
}
