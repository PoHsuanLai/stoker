//! One connection: the `Hello` exchange and one utterance's pump.

use std::pin::Pin;

use model_provider::{Flow, ProviderError};
use serde::Serialize;
use speech_provider::{
    AudioPull, AudioSource, HostIn, HostOut, HostVocab, SpeechModelInfo, SttEnd, SttRequest,
    TranscriptSink,
};
use tokio::net::UnixStream;

use crate::framed::{self, FrameReader};

/// Where the write side is, which decides whether a `Cancel` may follow it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Position {
    /// Between frames: a `Cancel` is a whole frame.
    Between,
    /// A write may be half done (the future was dropped in it): nothing more is written.
    InFrame,
    /// The host has answered the end; nothing more is owed.
    Over,
}

/// The write side of a connection. Dropped before the host's answer, it says `Cancel` if that
/// can be done without waiting; closing the socket follows either way.
pub struct Wire<'a> {
    stream: &'a UnixStream,
    position: Position,
}

impl<'a> Wire<'a> {
    pub fn new(stream: &'a UnixStream) -> Self {
        Self {
            stream,
            position: Position::Between,
        }
    }

    pub async fn send<T: Serialize>(&mut self, message: &T) -> Result<(), ProviderError> {
        self.position = Position::InFrame;
        let sent = framed::write(self.stream, message).await;
        self.position = Position::Between;
        sent
    }

    /// The host has answered; no `Cancel` on drop.
    pub fn finish(&mut self) {
        self.position = Position::Over;
    }
}

impl Drop for Wire<'_> {
    fn drop(&mut self) {
        if self.position != Position::Between {
            return;
        }
        // Best effort: a full socket buffer or a closed peer just means no `Cancel`.
        if let Ok(frame) = speech_provider::encode_frame(&HostIn::Cancel) {
            let _ = self.stream.try_write(&frame);
        }
    }
}

/// Sends our `Hello`, reads the host's, checks the vocabulary and returns its models.
pub async fn hello(
    wire: &mut Wire<'_>,
    reader: &mut FrameReader,
) -> Result<Vec<SpeechModelInfo>, ProviderError> {
    wire.send(&HostIn::Hello {
        vocab: HostVocab::CURRENT,
    })
    .await?;
    match reader.next(wire.stream).await? {
        HostOut::Hello { vocab, models } if vocab == HostVocab::CURRENT => Ok(models),
        HostOut::Hello { vocab, .. } => Err(ProviderError::Unreadable(format!(
            "the host speaks vocabulary {}, not {}",
            vocab.0,
            HostVocab::CURRENT.0
        ))),
        HostOut::Failed(error) => Err(error),
        HostOut::Event(_) | HostOut::Done(_) => Err(ProviderError::Unreadable(
            "the host did not start with Hello".to_owned(),
        )),
    }
}

/// Whether the sink still wants events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Listening {
    Yes,
    /// The sink said stop: `End` is sent, events are read and dropped until `Done`.
    No,
}

type Pull<'s> = Option<Pin<Box<dyn Future<Output = AudioPull> + Send + 's>>>;

async fn pulled(source: &mut Pull<'_>) -> AudioPull {
    match source {
        Some(pull) => pull.as_mut().await,
        None => std::future::pending().await,
    }
}

pub async fn utterance<A: AudioSource, K: TranscriptSink>(
    stream: &UnixStream,
    request: SttRequest,
    audio: &mut A,
    sink: &mut K,
) -> Result<SttEnd, ProviderError> {
    let mut wire = Wire::new(stream);
    let mut reader = FrameReader::default();
    hello(&mut wire, &mut reader).await?;
    wire.send(&HostIn::Begin(request)).await?;
    let mut listening = Listening::Yes;
    let mut source: Pull<'_> = Some(Box::pin(audio.next()));
    loop {
        let step = tokio::select! {
            pull = pulled(&mut source) => Step::Audio(pull),
            frame = reader.next(stream) => Step::Host(frame),
        };
        match step {
            Step::Audio(pull) => {
                source = None;
                let sent = match pull {
                    AudioPull::Chunk(chunk) => {
                        let sent = wire.send(&HostIn::Audio(chunk)).await;
                        source = Some(Box::pin(audio.next()));
                        sent
                    }
                    AudioPull::End => wire.send(&HostIn::End).await,
                };
                if let Err(error) = sent {
                    // A host that failed and hung up says why before it goes.
                    return Err(match reader.next(stream).await {
                        Ok(HostOut::Failed(why)) => why,
                        _ => error,
                    });
                }
            }
            Step::Host(frame) => match frame? {
                HostOut::Event(event) if listening == Listening::Yes => {
                    if sink.event(event) == Flow::Stop {
                        listening = Listening::No;
                        if source.take().is_some() {
                            wire.send(&HostIn::End).await?;
                        }
                    }
                }
                HostOut::Event(_) => {}
                HostOut::Done(end) => {
                    wire.finish();
                    return Ok(end);
                }
                HostOut::Failed(error) => {
                    wire.finish();
                    return Err(error);
                }
                HostOut::Hello { .. } => {
                    return Err(ProviderError::Unreadable(
                        "the host said Hello again mid-utterance".to_owned(),
                    ));
                }
            },
        }
    }
}

enum Step {
    Audio(AudioPull),
    Host(Result<HostOut, ProviderError>),
}
