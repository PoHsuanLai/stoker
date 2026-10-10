//! Retry and backoff, done once, in a wrapper.
//!
//! Rig only classifies a failure and leaves the loop to the host. Here `ProviderError` says how
//! it may be retried, `next_wait` is a pure function (the jitter is an input, so tests need no
//! random source), and [`Retrying`] runs the loop over a `Sleeper` (so tests need no clock).
//!
//! The rule that matters: never retry once the first event reached the sink. A half-delivered
//! turn is not idempotent for the planner.

use serde::{Deserialize, Serialize};

use crate::{
    Attempt, Flow, ModelInfo, Permille, Provider, ProviderError, RetrySeconds, TurnEnd, TurnEvent,
    TurnRequest, TurnSink, WaitMs,
};

/// How a failure may be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
pub enum RetryClass {
    Never,
    /// A transient fault: try again after the backoff.
    Transient,
    /// The server is overloaded and said how long to wait.
    Overload(RetrySeconds),
}

impl ProviderError {
    /// 408, 425, 429 and 5xx are retryable (`Server`, `RateLimited`); a refusal never is; an
    /// unreachable or not-ready engine and a timeout are transient; the rest are not.
    pub fn retry_class(&self) -> RetryClass {
        match self {
            ProviderError::RateLimited(wait) => RetryClass::Overload(*wait),
            ProviderError::Server(status) if retryable_status(status.0) => RetryClass::Transient,
            ProviderError::Unreachable | ProviderError::NotReady | ProviderError::Timeout => {
                RetryClass::Transient
            }
            ProviderError::Server(_)
            | ProviderError::Unauthorized
            | ProviderError::AuthRejected(_)
            | ProviderError::PaymentRequired(_)
            | ProviderError::ContextOverflow { .. }
            | ProviderError::BadRequest(_)
            | ProviderError::Refused(_)
            | ProviderError::Unreadable(_) => RetryClass::Never,
        }
    }
}

/// 408 (request timeout), 425 (too early), 429 (too many requests) and every 5xx.
fn retryable_status(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 500..=599)
}

/// How many tries, and the backoff between them (`base * 2^(n-1)`, capped at `cap`). The numbers
/// are settings the daemon reads; no default lives here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub attempts: Attempt,
    pub base: WaitMs,
    pub cap: WaitMs,
}

/// How long to wait before try `attempt + 1`, or `None` when the class or the budget says stop.
/// `jitter` scales the wait down by up to that share, so callers spread out.
pub fn next_wait(
    policy: &RetryPolicy,
    attempt: Attempt,
    class: RetryClass,
    jitter: Permille,
) -> Option<WaitMs> {
    if class == RetryClass::Never || attempt.0 >= policy.attempts.0 {
        return None;
    }
    let doubled = u64::from(policy.base.0) << u32::from(attempt.0.saturating_sub(1)).min(31);
    let capped = doubled.min(u64::from(policy.cap.0));
    let jitter = u64::from(jitter.0.min(1000));
    let backoff = capped - capped * jitter / 1000;
    let wait = match class {
        RetryClass::Overload(server) => backoff.max(u64::from(server.0) * 1000),
        RetryClass::Never | RetryClass::Transient => backoff,
    };
    Some(WaitMs(u32::try_from(wait).unwrap_or(u32::MAX)))
}

/// Where the waiting happens: a tokio timer in a daemon, a recorder in a test.
pub trait Sleeper: Send + Sync {
    fn sleep(&self, wait: WaitMs) -> impl Future<Output = ()> + Send;

    /// How much of each backoff to shave off (thousandths), so callers spread out: a daemon
    /// draws a random one, a test returns a fixed one. The default is none.
    fn jitter(&self) -> Permille {
        Permille(0)
    }
}

/// Wraps a provider and retries a turn that failed before it delivered an event. `describe` is
/// retried the same way (it has no events).
#[derive(Debug, Clone)]
pub struct Retrying<P: Provider, S: Sleeper> {
    inner: P,
    policy: RetryPolicy,
    sleep: S,
}

impl<P: Provider, S: Sleeper> Retrying<P, S> {
    pub fn new(inner: P, policy: RetryPolicy, sleep: S) -> Self {
        Self {
            inner,
            policy,
            sleep,
        }
    }
}

impl<P: Provider, S: Sleeper> Provider for Retrying<P, S> {
    async fn describe(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let mut attempt = Attempt(1);
        loop {
            match self.inner.describe().await {
                Err(error) => self.pause(attempt, &error).await?,
                done => return done,
            }
            attempt = Attempt(attempt.0.saturating_add(1));
        }
    }

    async fn turn<K: TurnSink>(
        &self,
        request: &TurnRequest,
        sink: &mut K,
    ) -> Result<TurnEnd, ProviderError> {
        let mut counting = Counting {
            inner: sink,
            seen: 0,
        };
        let mut attempt = Attempt(1);
        loop {
            match self.inner.turn(request, &mut counting).await {
                Err(error) if counting.seen == 0 => self.pause(attempt, &error).await?,
                done => return done,
            }
            attempt = Attempt(attempt.0.saturating_add(1));
        }
    }
}

impl<P: Provider, S: Sleeper> Retrying<P, S> {
    /// Sleeps the wait that follows try `attempt`, or hands `error` back when it may not be
    /// retried. The jitter comes from the sleeper, which owns the clock and any random source.
    async fn pause(&self, attempt: Attempt, error: &ProviderError) -> Result<(), ProviderError> {
        match next_wait(
            &self.policy,
            attempt,
            error.retry_class(),
            self.sleep.jitter(),
        ) {
            Some(wait) => {
                self.sleep.sleep(wait).await;
                Ok(())
            }
            None => Err(error.clone()),
        }
    }
}

/// Counts the events that reach the caller's sink: a turn that has delivered one is never retried.
struct Counting<'a, K> {
    inner: &'a mut K,
    seen: usize,
}

impl<K: TurnSink> TurnSink for Counting<'_, K> {
    fn event(&mut self, event: TurnEvent) -> Flow {
        self.seen += 1;
        self.inner.event(event)
    }
}
