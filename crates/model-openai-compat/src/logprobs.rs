//! First-token log-probabilities on the chat-completions stream: `logprobs: true` and
//! `top_logprobs: k` on the request, `choices[0].logprobs.content[].top_logprobs` on the chunks
//! (the same shape on vLLM and on llama-server's OpenAI endpoint).
//!
//! The first token belongs to the answer only when it is the token that opened the answer text:
//! thinking (`reasoning_content`) arrives in earlier chunks, and a chunk that mixes thinking and
//! answer, or whose first logged token is not the start of the answer text (a `</think>` marker,
//! a byte-fallback piece), cannot be attributed and gives nothing. Never an error.

use model_provider::{ChoiceScores, FirstTokenLogprobs, Logprob, TokenLogprob};
use serde_json::{Map, Value, json};

use crate::Flavor;

/// The most candidates read from one chunk: a hostile stream cannot grow the record.
const TOP_MAX: usize = 256;

/// Whether the flavor's server takes `logprobs` on a chat request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogprobsAsk {
    Request,
    /// Hosted gateways: not asked, so no reply carries them.
    Unsupported,
}

/// The request fields for `ask`: `top_k` is held to the 1 to 20 both engines accept.
pub(crate) fn encode(body: &mut Map<String, Value>, ask: ChoiceScores, flavor: Flavor) {
    let ChoiceScores::FirstToken { top_k } = ask else {
        return;
    };
    if flavor.quirks().logprobs == LogprobsAsk::Unsupported {
        return;
    }
    body.insert("logprobs".into(), json!(true));
    body.insert("top_logprobs".into(), json!(top_k.0.clamp(1, 20)));
}

/// Why a stream gave no first-token record, for the debug line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Miss {
    NoLogprobs,
    ThoughtInSameChunk,
    NotTheAnswersFirstToken,
    NoUsableCandidates,
}

/// Reads the answer's first token once, at the first chunk with answer text.
#[derive(Debug, Clone, Default)]
pub(crate) enum FirstToken {
    #[default]
    Waiting,
    Settled(Option<FirstTokenLogprobs>),
}

/// Whether the delta that carried the answer's first text also carried thinking text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Thinking {
    WithAnswer,
    Absent,
}

impl FirstToken {
    /// `choice` is the chunk's choice; `answer` its non-empty answer text; `thinking` whether the
    /// same delta carried thinking text.
    pub(crate) fn see(&mut self, choice: &Value, answer: &str, thinking: Thinking) {
        if !matches!(self, FirstToken::Waiting) {
            return;
        }
        let read = match thinking {
            Thinking::WithAnswer => Err(Miss::ThoughtInSameChunk),
            Thinking::Absent => read(choice, answer),
        };
        *self = FirstToken::Settled(match read {
            Ok(record) => Some(record),
            Err(miss) => {
                log::debug!("no first-token log-probabilities: {miss:?}");
                None
            }
        });
    }

    pub(crate) fn into_record(self) -> Option<FirstTokenLogprobs> {
        match self {
            FirstToken::Waiting => None,
            FirstToken::Settled(record) => record,
        }
    }
}

fn read(choice: &Value, answer: &str) -> Result<FirstTokenLogprobs, Miss> {
    let first = choice
        .get("logprobs")
        .and_then(|l| l.get("content"))
        .and_then(Value::as_array)
        .and_then(|content| content.first())
        .ok_or(Miss::NoLogprobs)?;
    let token = first.get("token").and_then(Value::as_str).unwrap_or("");
    if token.is_empty() || !answer.starts_with(token) {
        return Err(Miss::NotTheAnswersFirstToken);
    }
    let top: Vec<TokenLogprob> = first
        .get("top_logprobs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(TOP_MAX)
        .filter_map(candidate)
        .collect();
    if top.is_empty() {
        return Err(Miss::NoUsableCandidates);
    }
    Ok(FirstTokenLogprobs { top })
}

fn candidate(entry: &Value) -> Option<TokenLogprob> {
    let token = entry.get("token").and_then(Value::as_str)?;
    let logprob = Logprob::from_nats(entry.get("logprob").and_then(Value::as_f64)?)?;
    Some(TokenLogprob {
        token: token.to_owned(),
        logprob,
    })
}
