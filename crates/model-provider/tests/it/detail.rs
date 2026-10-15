//! `ProviderDetail`: the provider's status and message survive; credentials and length do not.

use model_provider::{ProviderDetail, ProviderError, ServerStatus, redact_excerpt};

#[test]
fn a_message_keeps_its_words_and_loses_its_credentials() {
    const CASES: &[(&str, &str)] = &[
        (
            "You requested up to 65536 tokens, but can only afford 367.",
            "You requested up to 65536 tokens, but can only afford 367.",
        ),
        ("bad key sk-abcdef123456 given", "bad key [redacted] given"),
        (
            "Authorization: Bearer abc.def.ghi failed",
            "Authorization: [redacted] [redacted] failed",
        ),
        ("send api_key=hunter2 again", "send [redacted] again"),
        ("token: hunter2 is wrong", "token: [redacted] is wrong"),
        (
            "jwt eyJhbGciOiJIUzI1NiJ9.payload.sig here",
            "jwt [redacted] here",
        ),
        (
            "key 0123456789abcdef0123456789abcdef used",
            "key [redacted] used",
        ),
        (
            "unsupported parameter response_format_json_schema",
            "unsupported parameter response_format_json_schema",
        ),
        ("two\nlines\tand   gaps", "two lines and gaps"),
        ("", ""),
    ];
    for (raw, want) in CASES {
        assert_eq!(&redact_excerpt(raw), want, "{raw}");
    }
}

#[test]
fn an_excerpt_is_cut_to_a_bound() {
    let long = "word ".repeat(200);
    let cut = redact_excerpt(&long);
    assert_eq!(cut.chars().count(), 241);
    assert!(cut.ends_with('…'));
    // A token that straddles the cut is redacted whole, not cut in half.
    let straddle = format!("{}sk-{}", "x ".repeat(118), "z".repeat(60));
    let cut = redact_excerpt(&straddle);
    assert!(!cut.contains("sk-"), "{cut}");
}

#[test]
fn a_detail_displays_the_status_only_and_keeps_the_message_for_logs() {
    let with = ProviderDetail::new(402, "can only afford 367 tokens");
    assert_eq!(with.status, ServerStatus(402));
    assert_eq!(with.message, "can only afford 367 tokens");
    assert_eq!(with.to_string(), "http 402");
    let error = ProviderError::PaymentRequired(with);
    assert_eq!(error.to_string(), "the account is out of credit (http 402)");
    assert_eq!(error.provider_message(), Some("can only afford 367 tokens"));
    assert_eq!(
        ProviderError::PaymentRequired(ProviderDetail::new(402, "")).provider_message(),
        None
    );
}

#[test]
fn a_bad_request_displays_its_slug_and_keeps_the_provider_message_for_logs() {
    let error = ProviderError::bad_request_with_message(
        "http_400 invalid_request_error",
        "Invalid 'messages[3]': my secret mail",
    );
    assert_eq!(
        error.to_string(),
        "the endpoint rejected the request: http_400 invalid_request_error"
    );
    assert_eq!(
        error.provider_message(),
        Some("Invalid 'messages[3]': my secret mail")
    );
    let bare = ProviderError::bad_request_with_message("http_404", "");
    assert_eq!(bare, ProviderError::BadRequest("http_404".into()));
    assert_eq!(bare.provider_message(), None);
    assert_eq!(ProviderError::Timeout.provider_message(), None);
}
