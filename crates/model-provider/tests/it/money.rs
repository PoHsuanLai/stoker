//! `Money`: exact sums of one currency, typed refusals, and a stable JSON form.

use iso_currency::Currency;
use model_provider::{Charges, Money, MoneyError, TurnUsage};
use rust_decimal::Decimal;

#[test]
fn amounts_of_one_currency_add_exactly() {
    let a = Money::usd(Decimal::new(1, 1));
    let b = Money::usd(Decimal::new(2, 1));
    assert_eq!(a.checked_add(b), Ok(Money::usd(Decimal::new(3, 1))));
}

#[test]
fn different_currencies_and_overflow_are_refused() {
    let usd = Money::usd(Decimal::ONE);
    let eur = Money::new(Decimal::ONE, Currency::EUR);
    assert_eq!(usd.checked_add(eur), Err(MoneyError::CurrencyMismatch));
    let top = Money::usd(Decimal::MAX);
    assert_eq!(top.checked_add(usd), Err(MoneyError::Overflow));
}

#[test]
fn a_usage_without_a_charge_writes_no_charge_field() {
    let json = serde_json::to_string(&TurnUsage::default()).unwrap();
    assert!(!json.contains("charge"), "{json}");
    let priced = TurnUsage {
        charge: Some(Money::usd(Decimal::new(95, 5))),
        ..TurnUsage::default()
    };
    let back: TurnUsage = serde_json::from_str(&serde_json::to_string(&priced).unwrap()).unwrap();
    assert_eq!(back, priced);
}

#[test]
fn charges_slugs_are_snake_case() {
    assert_eq!(
        serde_json::to_string(&Charges::NotReported).unwrap(),
        r#""not_reported""#
    );
}
