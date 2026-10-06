//! The host_wire state machine, pure: one message in, the replies out.

use model_provider::ProviderError;
use speech_provider::{HostIn, HostOut, HostVocab};

use crate::Recognizer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// No `Hello` yet.
    Greeting,
    Idle,
    /// An utterance is running.
    Running,
}

/// One connection's state over a recognizer.
#[derive(Debug)]
pub struct Session<'r, R: Recognizer> {
    recognizer: &'r mut R,
    phase: Phase,
}

fn refuse(why: &str) -> Vec<HostOut> {
    vec![HostOut::Failed(ProviderError::BadRequest(why.to_string()))]
}

impl<'r, R: Recognizer> Session<'r, R> {
    pub fn new(recognizer: &'r mut R) -> Self {
        Self {
            recognizer,
            phase: Phase::Greeting,
        }
    }

    /// Handles one message from inferd and returns what to write back, in order.
    pub fn step(&mut self, message: HostIn) -> Vec<HostOut> {
        match (self.phase, message) {
            (Phase::Greeting, HostIn::Hello { vocab }) => self.hello(vocab),
            (Phase::Greeting, _) => refuse("hello comes first"),
            (_, HostIn::Hello { .. }) => refuse("hello was already exchanged"),
            (Phase::Idle, HostIn::Begin(request)) => match self.recognizer.begin(&request) {
                Ok(()) => {
                    self.phase = Phase::Running;
                    Vec::new()
                }
                Err(error) => {
                    self.recognizer.reset();
                    vec![HostOut::Failed(error)]
                }
            },
            // The running utterance is untouched: one at a time.
            (Phase::Running, HostIn::Begin(_)) => refuse("an utterance is already running"),
            (Phase::Running, HostIn::Audio(chunk)) => match self.recognizer.accept(&chunk) {
                Ok(events) => events.into_iter().map(HostOut::Event).collect(),
                Err(error) => self.fail(error),
            },
            (Phase::Running, HostIn::End) => match self.recognizer.finish() {
                Ok(done) => {
                    self.recognizer.reset();
                    self.phase = Phase::Idle;
                    let mut out: Vec<HostOut> =
                        done.events.into_iter().map(HostOut::Event).collect();
                    out.push(HostOut::Done(done.end));
                    out
                }
                Err(error) => self.fail(error),
            },
            // Cancel is silent: the utterance has no transcript to report.
            (_, HostIn::Cancel) => {
                self.drop_utterance();
                Vec::new()
            }
            (Phase::Idle, HostIn::Audio(_) | HostIn::End) => refuse("no utterance is running"),
        }
    }

    /// The connection ended: nothing of the utterance is kept.
    pub fn close(&mut self) {
        self.drop_utterance();
    }

    fn hello(&mut self, vocab: HostVocab) -> Vec<HostOut> {
        if vocab != HostVocab::CURRENT {
            return refuse("unsupported host vocabulary");
        }
        self.phase = Phase::Idle;
        vec![HostOut::Hello {
            vocab: HostVocab::CURRENT,
            models: self.recognizer.models(),
        }]
    }

    fn fail(&mut self, error: ProviderError) -> Vec<HostOut> {
        self.drop_utterance();
        vec![HostOut::Failed(error)]
    }

    fn drop_utterance(&mut self) {
        self.recognizer.reset();
        if self.phase == Phase::Running {
            self.phase = Phase::Idle;
        }
    }
}
