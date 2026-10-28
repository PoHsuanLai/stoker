//! The charge an OpenRouter reply carries: `usage.cost`, in credits (1 credit is 1 US dollar),
//! always present in the stream chunk that carries `usage`, or in the whole body of a reply that
//! does not stream. The same field sits in the OpenAI and Anthropic shapes OpenRouter serves.

use model_provider::Money;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::value::RawValue;

#[derive(Deserialize)]
struct Frame<'a> {
    #[serde(borrow)]
    usage: Option<UsageText<'a>>,
}

#[derive(Deserialize)]
struct UsageText<'a> {
    #[serde(borrow)]
    cost: Option<&'a RawValue>,
}

/// The charge in a stream chunk or a whole reply body, in US dollars, read from the number's own
/// text so no float rounds it.
/// Absent, not a number, negative or too large for a decimal is `None`: a charge that cannot be
/// read is a missing charge, never a failed turn.
pub fn read_charge(frame: &str) -> Option<Money> {
    let parsed: Frame<'_> = serde_json::from_str(frame).ok()?;
    let text = parsed.usage?.cost?.get();
    let amount = Decimal::from_str_exact(text)
        .or_else(|_| Decimal::from_scientific(text))
        .ok()?;
    (!amount.is_sign_negative()).then(|| Money::usd(amount))
}
