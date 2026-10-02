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
    Attempt, ModelInfo, Permille, Provider, ProviderError, RetrySeconds, TurnEnd, TurnRequest,
    TurnSink, WaitMs,
};

/// How a failure may be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
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
        todo!("ProviderError::retry_class: rig's retryable_status and transient_transport, ported")
    }
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
    let _ = (policy, attempt, class, jitter);
    todo!("next_wait: exponent clamped at 31, cap, Overload takes the larger of server and backoff")
}

/// Where the waiting happens: a tokio timer in a daemon, a recorder in a test.
pub trait Sleeper: Send + Sync {
    fn sleep(&self, wait: WaitMs) -> impl Future<Output = ()> + Send;
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
    fn describe(&self) -> impl Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send {
        let _ = (&self.inner, &self.policy, &self.sleep);
        async { todo!("Retrying::describe: loop on retry_class and next_wait") }
    }

    fn turn<K: TurnSink>(
        &self,
        request: &TurnRequest,
        sink: &mut K,
    ) -> impl Future<Output = Result<TurnEnd, ProviderError>> + Send {
        let _ = (&self.inner, &self.policy, &self.sleep, request, &mut *sink);
        async { todo!("Retrying::turn: a counting sink; retry only while it has seen no event") }
    }
}
