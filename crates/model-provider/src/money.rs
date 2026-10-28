//! An amount of money a provider reported, and which providers report one.

use iso_currency::Currency;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// An exact amount in one currency. It carries only what a provider said: nothing here is
/// computed from a price table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub amount: Decimal,
    pub currency: Currency,
}

/// Why two amounts could not be added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MoneyError {
    #[error("the amounts are in different currencies")]
    CurrencyMismatch,
    #[error("the sum is too large to hold")]
    Overflow,
}

impl Money {
    pub fn new(amount: Decimal, currency: Currency) -> Self {
        Self { amount, currency }
    }

    /// An amount in US dollars (OpenRouter's credits are dollars).
    pub fn usd(amount: Decimal) -> Self {
        Self::new(amount, Currency::USD)
    }

    /// The sum of two amounts of the same currency; never a panic.
    pub fn checked_add(self, other: Money) -> Result<Money, MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch);
        }
        self.amount
            .checked_add(other.amount)
            .map(|amount| Money::new(amount, self.currency))
            .ok_or(MoneyError::Overflow)
    }
}

/// Whether a provider says what each reply cost. Only what the provider reports is ever shown;
/// there is no price table to fall back on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Charges {
    /// Every reply carries its charge (`TurnUsage::charge`), though one may still arrive without.
    PerReply,
    /// The provider reports tokens only (or nothing); `TurnUsage::charge` is always `None`.
    NotReported,
}
