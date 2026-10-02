use speech_provider::{AudioMs, SampleIndex};
use speech_vad::{EndWhy, Endpoint, EndpointParams, EnergyGateParams, FrameCount, Level};

#[test]
fn endpoint_states_have_pinned_json() {
    const CASES: &[(Endpoint, &str)] = &[
        (Endpoint::Waiting, r#"{"kind":"waiting"}"#),
        (
            Endpoint::InSpeech {
                since: SampleIndex(160),
            },
            r#"{"kind":"in_speech","v":{"since":160}}"#,
        ),
        (
            Endpoint::Trailing {
                since: SampleIndex(9600),
            },
            r#"{"kind":"trailing","v":{"since":9600}}"#,
        ),
        (
            Endpoint::Ended(EndWhy::NoSpeech),
            r#"{"kind":"ended","v":"no_speech"}"#,
        ),
        (
            Endpoint::Ended(EndWhy::Silence),
            r#"{"kind":"ended","v":"silence"}"#,
        ),
    ];
    for (state, json) in CASES {
        assert_eq!(&serde_json::to_string(state).unwrap(), json);
        assert_eq!(&serde_json::from_str::<Endpoint>(json).unwrap(), state);
    }
}

#[test]
fn proposed_defaults_are_the_specs() {
    let p = EndpointParams::default();
    assert_eq!(
        (p.lead, p.silence_end, p.min_speech),
        (AudioMs(300), AudioMs(30_000), AudioMs(150))
    );
    let g = EnergyGateParams::default();
    assert_eq!((g.threshold, g.hangover), (Level(333), FrameCount(8)));
}

#[test]
fn params_round_trip() {
    let p = EndpointParams::default();
    let json = serde_json::to_string(&p).unwrap();
    assert_eq!(json, r#"{"lead":300,"silence_end":30000,"min_speech":150}"#);
    assert_eq!(serde_json::from_str::<EndpointParams>(&json).unwrap(), p);
}
