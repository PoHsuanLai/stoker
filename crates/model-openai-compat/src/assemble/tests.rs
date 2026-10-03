use std::collections::BTreeSet;

use model_provider::{CallIndex, TurnEvent};
use model_wire::CodecError;

use super::{Closed, Fragment, IfMalformed, Pending};

fn open(args: &str) -> Pending {
    let mut pending = Pending::new(CallIndex(0));
    let fragment = Fragment {
        id: Some("a".into()),
        name: Some("f".into()),
        arguments: (!args.is_empty()).then(|| args.to_owned()),
    };
    pending.absorb(fragment, &mut BTreeSet::new()).unwrap();
    pending
}

fn input(closed: Result<Closed, CodecError>) -> Option<String> {
    match closed {
        Ok(Closed::Done(call)) => Some(call.input.as_str().to_owned()),
        _ => None,
    }
}

#[test]
fn closing_a_well_formed_call_ignores_the_policy() {
    for how in [
        IfMalformed::Fail,
        IfMalformed::EmptyObject,
        IfMalformed::Drop,
        IfMalformed::KeepOpen,
    ] {
        assert_eq!(
            input(open("{\"x\":1}").close(how)).as_deref(),
            Some("{\"x\":1}"),
            "{how:?}"
        );
    }
}

#[test]
fn the_four_outcomes_for_arguments_that_are_not_json() {
    assert_eq!(
        open("{\"x\":").close(IfMalformed::Fail),
        Err(CodecError::BadToolArguments)
    );
    assert_eq!(
        input(open("{\"x\":").close(IfMalformed::EmptyObject)).as_deref(),
        Some("{}")
    );
    assert_eq!(
        open("{\"x\":").close(IfMalformed::Drop),
        Ok(Closed::Dropped)
    );
    assert_eq!(
        open("{\"x\":").close(IfMalformed::KeepOpen),
        Ok(Closed::Open)
    );
}

#[test]
fn a_call_that_never_got_a_name_is_dropped_or_a_fault() {
    let mut pending = Pending::new(CallIndex(0));
    let events = pending
        .absorb(
            Fragment {
                arguments: Some("{}".into()),
                ..Fragment::default()
            },
            &mut BTreeSet::new(),
        )
        .unwrap();
    assert_eq!(events, Vec::<TurnEvent>::new());
    assert_eq!(
        pending.clone().close(IfMalformed::Drop),
        Ok(Closed::Dropped)
    );
    assert_eq!(
        pending.clone().close(IfMalformed::EmptyObject),
        Ok(Closed::Dropped)
    );
    assert_eq!(
        pending.clone().close(IfMalformed::KeepOpen),
        Ok(Closed::Open)
    );
    assert_eq!(
        pending.close(IfMalformed::Fail),
        Err(CodecError::BadToolArguments)
    );
}
