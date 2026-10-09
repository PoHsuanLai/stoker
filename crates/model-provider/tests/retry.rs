//! Retry classification, `next_wait` and `Retrying`.

use std::future::Future;
use std::pin::pin;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

use model_provider::{
    Attempt, Flow, ModelInfo, ModelName, Permille, Provider, ProviderError, RetryClass,
    RetryPolicy, RetrySeconds, Retrying, Script, ScriptedProvider, ServerStatus, Sleeper,
    StopReason, Tokens, TurnEnd, TurnEvent, TurnRequest, TurnSink, TurnUsage, WaitMs, next_wait,
};
use proptest::prelude::*;

fn block_on<T>(future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
    }
}

const POLICY: RetryPolicy = RetryPolicy {
    attempts: Attempt(4),
    base: WaitMs(100),
    cap: WaitMs(1000),
};

#[test]
fn errors_are_classified() {
    let cases: Vec<(ProviderError, RetryClass)> = vec![
        (ProviderError::Unreachable, RetryClass::Transient),
        (ProviderError::NotReady, RetryClass::Transient),
        (ProviderError::Timeout, RetryClass::Transient),
        (
            ProviderError::RateLimited(RetrySeconds(7)),
            RetryClass::Overload(RetrySeconds(7)),
        ),
        (
            ProviderError::Server(ServerStatus(500)),
            RetryClass::Transient,
        ),
        (
            ProviderError::Server(ServerStatus(503)),
            RetryClass::Transient,
        ),
        (
            ProviderError::Server(ServerStatus(599)),
            RetryClass::Transient,
        ),
        (
            ProviderError::Server(ServerStatus(408)),
            RetryClass::Transient,
        ),
        (
            ProviderError::Server(ServerStatus(425)),
            RetryClass::Transient,
        ),
        (
            ProviderError::Server(ServerStatus(429)),
            RetryClass::Transient,
        ),
        (ProviderError::Server(ServerStatus(400)), RetryClass::Never),
        (ProviderError::Server(ServerStatus(404)), RetryClass::Never),
        (ProviderError::Server(ServerStatus(600)), RetryClass::Never),
        (ProviderError::Unauthorized, RetryClass::Never),
        (
            ProviderError::ContextOverflow { limit: Tokens(1) },
            RetryClass::Never,
        ),
        (ProviderError::BadRequest("x".into()), RetryClass::Never),
        (ProviderError::Refused("x".into()), RetryClass::Never),
        (ProviderError::Unreadable("x".into()), RetryClass::Never),
    ];
    for (error, class) in cases {
        assert_eq!(error.retry_class(), class, "{error:?}");
    }
}

#[test]
fn the_wait_doubles_to_the_cap_and_the_budget_ends_it() {
    const CASES: &[(u8, RetryClass, u16, Option<u32>)] = &[
        (1, RetryClass::Transient, 0, Some(100)),
        (2, RetryClass::Transient, 0, Some(200)),
        (3, RetryClass::Transient, 0, Some(400)),
        (4, RetryClass::Transient, 0, None),
        (9, RetryClass::Transient, 0, None),
        (1, RetryClass::Never, 0, None),
        (1, RetryClass::Transient, 250, Some(75)),
        (1, RetryClass::Transient, 1000, Some(0)),
        (1, RetryClass::Transient, 60_000, Some(0)),
        (1, RetryClass::Overload(RetrySeconds(5)), 0, Some(5000)),
        (3, RetryClass::Overload(RetrySeconds(0)), 0, Some(400)),
        (1, RetryClass::Overload(RetrySeconds(1)), 500, Some(1000)),
        (
            1,
            RetryClass::Overload(RetrySeconds(u32::MAX)),
            0,
            Some(u32::MAX),
        ),
    ];
    for (attempt, class, jitter, want) in CASES {
        let got = next_wait(&POLICY, Attempt(*attempt), *class, Permille(*jitter));
        assert_eq!(
            got,
            want.map(WaitMs),
            "attempt {attempt} {class:?} jitter {jitter}"
        );
    }
}

#[test]
fn the_cap_holds_and_the_shift_cannot_overflow() {
    let wide = RetryPolicy {
        attempts: Attempt(255),
        base: WaitMs(u32::MAX),
        cap: WaitMs(5000),
    };
    for attempt in [1, 2, 31, 32, 33, 100, 254] {
        assert_eq!(
            next_wait(&wide, Attempt(attempt), RetryClass::Transient, Permille(0)),
            Some(WaitMs(5000))
        );
    }
    assert_eq!(
        next_wait(&wide, Attempt(255), RetryClass::Transient, Permille(0)),
        None
    );
    let zero = RetryPolicy {
        attempts: Attempt(0),
        ..POLICY
    };
    assert_eq!(
        next_wait(&zero, Attempt(1), RetryClass::Transient, Permille(0)),
        None
    );
}

proptest! {
    #[test]
    fn a_wait_never_exceeds_the_cap_unless_the_server_asked_for_more(
        attempt in 1u8..255, attempts in 1u8..255, base in any::<u32>(), cap in any::<u32>(),
        jitter in any::<u16>(), server in 0u32..100_000
    ) {
        let policy = RetryPolicy { attempts: Attempt(attempts), base: WaitMs(base), cap: WaitMs(cap) };
        let transient = next_wait(&policy, Attempt(attempt), RetryClass::Transient, Permille(jitter));
        prop_assert_eq!(transient.is_some(), attempt < attempts);
        if let Some(WaitMs(wait)) = transient { prop_assert!(wait <= cap); }
        let overload = next_wait(&policy, Attempt(attempt), RetryClass::Overload(RetrySeconds(server)), Permille(jitter));
        if let (Some(WaitMs(wait)), Some(WaitMs(plain))) = (overload, transient) {
            prop_assert!(wait >= plain && wait >= server * 1000);
        }
    }
}

#[derive(Default)]
struct Recorder(Mutex<Vec<WaitMs>>);

impl Sleeper for &Recorder {
    fn sleep(&self, wait: WaitMs) -> impl Future<Output = ()> + Send {
        self.0.lock().unwrap().push(wait);
        std::future::ready(())
    }
}

fn request() -> TurnRequest {
    use model_provider::{
        EngineExtras, Knob, Limits, Milli, OutputShape, Reasoning, Sampling, ToolChoice,
        ToolParallelism,
    };
    TurnRequest {
        model: ModelName("m".into()),
        messages: vec![],
        tools: vec![],
        tool_choice: ToolChoice::Auto,
        tool_calls: ToolParallelism::One,
        output: OutputShape::Free,
        limits: Limits {
            max_output: Tokens(8),
            stop: vec![],
        },
        sampling: Sampling {
            temperature: Milli(0),
            top_p: Knob::Off,
            top_k: Knob::Off,
            min_p: Knob::Off,
            repeat_penalty: Knob::Off,
            seed: Knob::Off,
        },
        reasoning: Reasoning::Off,
        engine: EngineExtras::None,
        choice_scores: model_provider::ChoiceScores::Off,
    }
}

fn end() -> TurnEnd {
    TurnEnd {
        stop: StopReason::EndTurn,
        usage: TurnUsage::default(),
        served: ModelName("m".into()),
        first_token: None,
    }
}

fn fail(error: ProviderError) -> Script {
    Script {
        events: vec![],
        end: Err(error),
    }
}

fn delta() -> Script {
    Script {
        events: vec![TurnEvent::TextDelta("hi".into())],
        end: Ok(end()),
    }
}

struct Keep(Vec<TurnEvent>);

impl TurnSink for Keep {
    fn event(&mut self, event: TurnEvent) -> Flow {
        self.0.push(event);
        Flow::Continue
    }
}

fn run(
    scripts: Vec<Script>,
) -> (
    Result<TurnEnd, ProviderError>,
    Vec<WaitMs>,
    usize,
    Vec<TurnEvent>,
) {
    let recorder = Recorder::default();
    let inner = ScriptedProvider::new(vec![], scripts);
    let retrying = Retrying::new(inner, POLICY, &recorder);
    let mut sink = Keep(vec![]);
    let result = block_on(retrying.turn(&request(), &mut sink));
    let waits = recorder.0.lock().unwrap().clone();
    (result, waits, 0, sink.0)
}

#[test]
fn a_turn_that_fails_before_any_event_is_retried_with_backoff() {
    let (result, waits, _, events) = run(vec![
        fail(ProviderError::Unreachable),
        fail(ProviderError::Server(ServerStatus(503))),
        delta(),
    ]);
    assert_eq!(result, Ok(end()));
    assert_eq!(waits, vec![WaitMs(100), WaitMs(200)]);
    assert_eq!(events, vec![TurnEvent::TextDelta("hi".into())]);
}

#[test]
fn retry_after_is_honoured() {
    let (result, waits, _, _) = run(vec![
        fail(ProviderError::RateLimited(RetrySeconds(3))),
        delta(),
    ]);
    assert_eq!(result, Ok(end()));
    assert_eq!(waits, vec![WaitMs(3000)]);
}

#[test]
fn a_turn_is_never_retried_after_its_first_event() {
    let half = Script {
        events: vec![TurnEvent::TextDelta("par".into())],
        end: Err(ProviderError::Timeout),
    };
    let (result, waits, _, events) = run(vec![half, delta()]);
    assert_eq!(result, Err(ProviderError::Timeout));
    assert!(waits.is_empty(), "no sleep and no second try");
    assert_eq!(events, vec![TurnEvent::TextDelta("par".into())]);
}

#[test]
fn a_refusal_or_a_bad_request_is_not_retried() {
    for error in [
        ProviderError::Unauthorized,
        ProviderError::BadRequest("x".into()),
        ProviderError::Refused("x".into()),
    ] {
        let (result, waits, _, _) = run(vec![fail(error.clone()), delta()]);
        assert_eq!(result, Err(error));
        assert!(waits.is_empty());
    }
}

#[test]
fn the_budget_ends_the_loop_with_the_last_error() {
    let (result, waits, _, _) = run(vec![
        fail(ProviderError::Timeout),
        fail(ProviderError::Timeout),
        fail(ProviderError::Timeout),
        fail(ProviderError::NotReady),
        delta(),
    ]);
    assert_eq!(result, Err(ProviderError::NotReady));
    assert_eq!(waits, vec![WaitMs(100), WaitMs(200), WaitMs(400)]);
}

#[test]
fn describe_is_retried_the_same_way() {
    struct Flaky(Mutex<u8>);
    impl Provider for Flaky {
        fn describe(&self) -> impl Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send {
            let mut tries = self.0.lock().unwrap();
            *tries += 1;
            let result = if *tries < 3 {
                Err(ProviderError::Unreachable)
            } else {
                Ok(vec![])
            };
            std::future::ready(result)
        }
        fn turn<K: TurnSink>(
            &self,
            _: &TurnRequest,
            _: &mut K,
        ) -> impl Future<Output = Result<TurnEnd, ProviderError>> + Send {
            std::future::ready(Err(ProviderError::NotReady))
        }
    }
    let recorder = Recorder::default();
    let retrying = Retrying::new(Flaky(Mutex::new(0)), POLICY, &recorder);
    assert_eq!(block_on(retrying.describe()), Ok(vec![]));
    assert_eq!(*recorder.0.lock().unwrap(), vec![WaitMs(100), WaitMs(200)]);
}

/// A sleeper that shaves a fixed share off every wait.
struct Jittery(Mutex<Vec<WaitMs>>, Permille);

impl Sleeper for &Jittery {
    fn sleep(&self, wait: WaitMs) -> impl Future<Output = ()> + Send {
        self.0.lock().unwrap().push(wait);
        std::future::ready(())
    }

    fn jitter(&self) -> Permille {
        self.1
    }
}

#[test]
fn the_sleepers_jitter_shaves_every_backoff() {
    let sleeper = Jittery(Mutex::new(vec![]), Permille(250));
    let inner = ScriptedProvider::new(
        vec![],
        vec![
            fail(ProviderError::Unreachable),
            fail(ProviderError::Unreachable),
            delta(),
        ],
    );
    let retrying = Retrying::new(inner, POLICY, &sleeper);
    let mut sink = Keep(vec![]);
    assert_eq!(block_on(retrying.turn(&request(), &mut sink)), Ok(end()));
    assert_eq!(
        *sleeper.0.lock().unwrap(),
        vec![WaitMs(75), WaitMs(150)],
        "a jittery sleeper shaves every backoff"
    );
    assert_eq!(
        (&Recorder::default()).jitter(),
        Permille(0),
        "a sleeper without a jitter source shaves nothing"
    );
}
