//! First-token log-probabilities: the raw material for the probability of each option of a
//! constrained choice, from the same call that answers it.
//!
//! The request side is [`ChoiceScores`]; the engine's answer is [`FirstTokenLogprobs`] on the
//! turn's end. The renormalisation over declared options is the caller's (porter's) job; the pure
//! helper [`FirstTokenLogprobs::option_permille`] states the rules once.

use serde::{Deserialize, Serialize};

use crate::{Count, Permille};

/// Whether a turn asks the engine for the log-probabilities of its first answer token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ChoiceScores {
    /// Nothing is asked: the request is what it always was.
    #[default]
    Off,
    /// The `top_k` most likely first tokens with their log-probabilities. Meant for
    /// `OutputShape::Choice`; a codec that cannot ask leaves the field out.
    FirstToken { top_k: Count },
}

/// A natural logarithm of a probability, in millionths of a nat. Integer, so a turn's end stays
/// `Eq`. `i32::MIN` is "impossible" (a `-inf` from the engine); positive input clamps to 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Logprob(pub i32);

impl Logprob {
    /// `None` for NaN. A `-inf` and anything below `i32::MIN` millionths become [`Logprob::NEVER`].
    pub fn from_nats(nats: f64) -> Option<Self> {
        if nats.is_nan() {
            return None;
        }
        // The cast saturates, which is the clamp meant.
        Some(Self((nats.min(0.0) * 1e6).round() as i32))
    }

    pub const NEVER: Self = Self(i32::MIN);

    /// The probability, 0.0 to 1.0.
    pub fn probability(self) -> f64 {
        if self == Self::NEVER {
            0.0
        } else {
            (f64::from(self.0) / 1e6).exp()
        }
    }
}

/// One candidate first token as the engine spelled it, and how likely the engine found it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TokenLogprob {
    pub token: String,
    pub logprob: Logprob,
}

/// The top-k candidates for the first token of the answer, in the engine's order (most likely
/// first). For a reasoning model it is the first token after the thinking; a codec that cannot
/// tell leaves it out.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct FirstTokenLogprobs {
    pub top: Vec<TokenLogprob>,
}

impl FirstTokenLogprobs {
    /// The share of each option in permille, in the order given, summing to exactly 1000 (largest
    /// remainder; ties go to the earlier option). A token counts for the option whose text starts
    /// with it (no tokenizer is at hand, so the token is taken to be that option's first). `None`
    /// when: there are fewer than two options; a token with probability mass starts two options
    /// (first-token scores cannot tell them apart); or no option has any mass in the top-k. An
    /// option absent from the top-k has mass 0.
    pub fn option_permille(&self, options: &[&str]) -> Option<Vec<Permille>> {
        if options.len() < 2 {
            return None;
        }
        let mut mass = vec![0.0_f64; options.len()];
        for entry in &self.top {
            let p = entry.logprob.probability();
            if entry.token.is_empty() || p <= 0.0 {
                continue;
            }
            let mut owners = options
                .iter()
                .enumerate()
                .filter(|(_, o)| o.starts_with(entry.token.as_str()));
            match (owners.next(), owners.next()) {
                (Some((i, _)), None) => mass[i] += p,
                (Some(_), Some(_)) => return None,
                (None, _) => {}
            }
        }
        let total: f64 = mass.iter().sum();
        if total <= 0.0 || !total.is_finite() {
            return None;
        }
        Some(largest_remainder(&mass, total))
    }
}

fn largest_remainder(mass: &[f64], total: f64) -> Vec<Permille> {
    let exact: Vec<f64> = mass.iter().map(|m| m / total * 1000.0).collect();
    let mut shares: Vec<u16> = exact.iter().map(|e| *e as u16).collect();
    let mut order: Vec<usize> = (0..exact.len()).collect();
    // Biggest remainder first; equal remainders keep declared order (the sort is stable).
    order.sort_by(|a, b| {
        let (ra, rb) = (exact[*a].fract(), exact[*b].fract());
        rb.partial_cmp(&ra).unwrap_or(core::cmp::Ordering::Equal)
    });
    let missing = 1000_u16.saturating_sub(shares.iter().sum());
    for i in order.into_iter().take(usize::from(missing)) {
        shares[i] += 1;
    }
    shares.into_iter().map(Permille).collect()
}
