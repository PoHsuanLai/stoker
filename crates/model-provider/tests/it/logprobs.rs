//! The first-token helper: renormalisation over declared options, in permille.

use model_provider::{FirstTokenLogprobs, Logprob, Permille, TokenLogprob};

fn top(entries: &[(&str, f64)]) -> FirstTokenLogprobs {
    FirstTokenLogprobs {
        top: entries
            .iter()
            .filter_map(|(token, nats)| {
                Some(TokenLogprob {
                    token: (*token).into(),
                    logprob: Logprob::from_nats(*nats)?,
                })
            })
            .collect(),
    }
}

fn shares(got: Option<Vec<Permille>>) -> Option<Vec<u16>> {
    got.map(|v| v.into_iter().map(|p| p.0).collect())
}

#[test]
fn shares_sum_to_exactly_one_thousand() {
    let got = top(&[
        ("pass", 0.7_f64.ln()),
        ("flag", 0.2_f64.ln()),
        ("maybe", 0.1_f64.ln()),
    ])
    .option_permille(&["pass", "flag", "maybe"]);
    let got = shares(got).unwrap();
    assert_eq!(got.iter().sum::<u16>(), 1000);
    assert_eq!(got, vec![700, 200, 100]);
}

#[test]
fn largest_remainder_settles_thirds_and_ties_go_to_the_earlier_option() {
    let third = (1.0_f64 / 3.0).ln();
    let got = top(&[("a", third), ("b", third), ("c", third)]).option_permille(&["a", "b", "c"]);
    assert_eq!(shares(got), Some(vec![334, 333, 333]));
}

#[test]
fn an_option_missing_from_the_top_k_counts_zero() {
    let got = top(&[("pass", 0.9_f64.ln())]).option_permille(&["pass", "flag"]);
    assert_eq!(shares(got), Some(vec![1000, 0]));
}

#[test]
fn every_option_missing_gives_none() {
    assert_eq!(
        top(&[("other", -0.1)]).option_permille(&["pass", "flag"]),
        None
    );
    assert_eq!(top(&[]).option_permille(&["pass", "flag"]), None);
}

#[test]
fn a_first_token_shared_by_two_options_gives_none() {
    let got =
        top(&[("pass", -0.5), ("flag", -1.0)]).option_permille(&["pass", "pass_through", "flag"]);
    assert_eq!(got, None);
}

#[test]
fn a_partial_token_that_starts_one_option_counts_for_it() {
    let got =
        top(&[("fl", 0.25_f64.ln()), ("pa", 0.75_f64.ln())]).option_permille(&["pass", "flag"]);
    assert_eq!(shares(got), Some(vec![750, 250]));
}

#[test]
fn negative_infinity_is_impossible_and_nan_is_dropped() {
    let entries = [
        ("pass", f64::NEG_INFINITY),
        ("flag", -0.2),
        ("maybe", f64::NAN),
    ];
    let got = top(&entries).option_permille(&["pass", "flag"]);
    assert_eq!(shares(got), Some(vec![0, 1000]));
    assert_eq!(Logprob::from_nats(f64::NAN), None);
    assert_eq!(Logprob::from_nats(f64::NEG_INFINITY), Some(Logprob::NEVER));
    assert_eq!(Logprob::from_nats(0.3), Some(Logprob(0)));
}

#[test]
fn fewer_than_two_options_give_none() {
    assert_eq!(top(&[("pass", -0.1)]).option_permille(&["pass"]), None);
}
