//! The client against a fake host on a loopback Unix socket, speaking `host_wire`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use model_provider::{Flow, ModelName, ProviderError, Support};
use speech_host_client::{HostSocket, SpeechHostClient};
use speech_provider::{
    AudioChunk, AudioFormat, AudioMs, AudioPull, AudioSource, HeardText, HostIn, HostOut,
    HostVocab, Lang, LangChoice, LangSet, MAX_FRAME_BYTES, PcmBytes, PcmFormat, SampleIndex,
    SampleRate, SpeechCaps, SpeechIo, SpeechModelInfo, SpeechToText, SttEnd, SttMode, SttRequest,
    TranscriptEvent, TranscriptSink, decode_frame, encode_frame,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};

/// A directory under TMPDIR, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "speech-host-client-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The host's side of one connection.
struct Conn(UnixStream);

impl Conn {
    async fn recv(&mut self) -> Option<HostIn> {
        let mut header = [0_u8; 4];
        self.0.read_exact(&mut header).await.ok()?;
        let mut body = vec![0_u8; u32::from_be_bytes(header) as usize];
        self.0.read_exact(&mut body).await.ok()?;
        Some(decode_frame(&body).expect("the client sends host_wire frames"))
    }

    async fn send(&mut self, message: &HostOut) {
        let frame = encode_frame(message).expect("encodes");
        self.0.write_all(&frame).await.expect("the client is listening");
    }

    async fn hello(&mut self) {
        assert_eq!(
            self.recv().await,
            Some(HostIn::Hello {
                vocab: HostVocab::CURRENT
            })
        );
        self.send(&HostOut::Hello {
            vocab: HostVocab::CURRENT,
            models: vec![model()],
        })
        .await;
    }

    async fn hello_and_begin(&mut self) {
        self.hello().await;
        assert_eq!(self.recv().await, Some(HostIn::Begin(request())));
    }
}

fn format() -> AudioFormat {
    AudioFormat {
        rate: SampleRate(16_000),
        pcm: PcmFormat::S16Le,
    }
}

fn model() -> SpeechModelInfo {
    SpeechModelInfo {
        name: ModelName("nemotron".to_owned()),
        caps: SpeechCaps {
            streaming: Support::Present,
            partials: Support::Present,
            punctuation: Support::Present,
            timestamps: Support::Absent,
            langs: LangSet::Any,
            max_audio: AudioMs(60_000),
            io: SpeechIo::In { input: format() },
        },
    }
}

fn request() -> SttRequest {
    SttRequest {
        model: ModelName("nemotron".to_owned()),
        mode: SttMode::Streaming { chunk: AudioMs(160) },
        lang: LangChoice::Prefer(vec![Lang::new("en").expect("a language tag")]),
        format: format(),
    }
}

fn chunk(at: u64) -> AudioChunk {
    AudioChunk {
        format: format(),
        at: SampleIndex(at),
        pcm: PcmBytes::new(vec![1, 2, 3, 4]),
    }
}

fn heard(text: &str) -> HeardText {
    HeardText(text.to_owned())
}

fn end() -> SttEnd {
    SttEnd {
        text: heard("hello world"),
        audio: AudioMs(1),
        served: ModelName("nemotron".to_owned()),
    }
}

/// Audio the test feeds by hand; a dropped sender is the end of the audio.
struct Fed(mpsc::Receiver<AudioChunk>);

impl AudioSource for Fed {
    async fn next(&mut self) -> AudioPull {
        self.0.recv().await.map_or(AudioPull::End, AudioPull::Chunk)
    }
}

/// Keeps every event; says stop after `limit` of them, if there is one.
struct Collect {
    events: Vec<TranscriptEvent>,
    limit: Option<usize>,
}

impl Collect {
    fn all() -> Self {
        Self {
            events: Vec::new(),
            limit: None,
        }
    }
}

impl TranscriptSink for Collect {
    fn event(&mut self, event: TranscriptEvent) -> Flow {
        self.events.push(event);
        match self.limit {
            Some(n) if self.events.len() >= n => Flow::Stop,
            _ => Flow::Continue,
        }
    }
}

/// A listening fake host; `serve` runs `script` on the first connection.
struct Fake {
    _dir: TempDir,
    client: SpeechHostClient,
    host: tokio::task::JoinHandle<()>,
}

fn fake<F, Fut>(script: F) -> Fake
where
    F: FnOnce(Conn) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let dir = TempDir::new();
    let path = dir.0.join("host.sock");
    let listener = UnixListener::bind(&path).expect("bind");
    let host = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        script(Conn(stream)).await;
    });
    Fake {
        _dir: dir,
        client: SpeechHostClient::new(HostSocket(path)),
        host,
    }
}

async fn run(fake: &Fake, audio: &mut Fed, sink: &mut Collect) -> Result<SttEnd, ProviderError> {
    fake.client.transcribe(&request(), audio, sink).await
}

#[tokio::test]
async fn describe_returns_the_models_the_host_names() {
    let fake = fake(|mut conn| async move { conn.hello().await });
    assert_eq!(fake.client.describe().await, Ok(vec![model()]));
    fake.host.await.expect("host script");
}

#[tokio::test]
async fn describe_refuses_another_vocabulary() {
    let fake = fake(|mut conn| async move {
        conn.recv().await;
        conn.send(&HostOut::Hello {
            vocab: HostVocab(99),
            models: Vec::new(),
        })
        .await;
    });
    assert!(matches!(
        fake.client.describe().await,
        Err(ProviderError::Unreadable(_))
    ));
}

#[tokio::test]
async fn describe_without_a_host_is_unreachable() {
    let dir = TempDir::new();
    let client = SpeechHostClient::new(HostSocket(dir.0.join("none.sock")));
    assert_eq!(client.describe().await, Err(ProviderError::Unreachable));
}

#[tokio::test]
async fn an_utterance_streams_audio_and_events_to_the_final() {
    let partial = TranscriptEvent::Partial {
        text: heard("hel"),
        from: SampleIndex(0),
    };
    let last = TranscriptEvent::Final {
        text: heard("hello world"),
        from: SampleIndex(0),
        to: SampleIndex(4),
    };
    let (p, l) = (partial.clone(), last.clone());
    let fake = fake(|mut conn| async move {
        conn.hello_and_begin().await;
        assert_eq!(conn.recv().await, Some(HostIn::Audio(chunk(0))));
        conn.send(&HostOut::Event(p)).await;
        assert_eq!(conn.recv().await, Some(HostIn::Audio(chunk(2))));
        assert_eq!(conn.recv().await, Some(HostIn::End));
        conn.send(&HostOut::Event(l)).await;
        conn.send(&HostOut::Done(end())).await;
        // A finished utterance is not cancelled: the client just closes.
        assert_eq!(conn.recv().await, None);
    });
    let (tx, rx) = mpsc::channel(4);
    tx.send(chunk(0)).await.expect("send");
    tx.send(chunk(2)).await.expect("send");
    drop(tx);
    let mut sink = Collect::all();
    assert_eq!(run(&fake, &mut Fed(rx), &mut sink).await, Ok(end()));
    assert_eq!(sink.events, vec![partial, last]);
    fake.host.await.expect("host script");
}

#[tokio::test]
async fn a_failed_frame_ends_the_utterance_with_its_error() {
    let fake = fake(|mut conn| async move {
        conn.hello_and_begin().await;
        assert_eq!(conn.recv().await, Some(HostIn::Audio(chunk(0))));
        conn.send(&HostOut::Failed(ProviderError::NotReady)).await;
    });
    let (tx, rx) = mpsc::channel(4);
    tx.send(chunk(0)).await.expect("send");
    let result = run(&fake, &mut Fed(rx), &mut Collect::all()).await;
    assert_eq!(result, Err(ProviderError::NotReady));
    fake.host.await.expect("host script");
    drop(tx);
}

#[tokio::test]
async fn a_host_that_closes_early_is_unreachable() {
    let fake = fake(|mut conn| async move {
        conn.hello_and_begin().await;
        // The connection drops with the utterance open.
    });
    let (tx, rx) = mpsc::channel(4);
    let result = run(&fake, &mut Fed(rx), &mut Collect::all()).await;
    assert_eq!(result, Err(ProviderError::Unreachable));
    drop(tx);
}

#[tokio::test]
async fn a_host_that_closes_while_audio_is_sent_is_unreachable() {
    let fake = fake(|mut conn| async move {
        conn.hello_and_begin().await;
    });
    let (tx, rx) = mpsc::channel(64);
    for at in 0..64 {
        tx.send(chunk(at)).await.expect("send");
    }
    drop(tx);
    let result = run(&fake, &mut Fed(rx), &mut Collect::all()).await;
    assert_eq!(result, Err(ProviderError::Unreachable));
}

#[tokio::test]
async fn a_frame_over_the_limit_is_unreadable() {
    let fake = fake(|mut conn| async move {
        conn.hello_and_begin().await;
        let len = u32::try_from(MAX_FRAME_BYTES + 1).expect("fits");
        conn.0.write_all(&len.to_be_bytes()).await.expect("write");
        // Keep the connection open: the header alone is refused.
        let _ = conn.recv().await;
    });
    let (tx, rx) = mpsc::channel(4);
    let result = run(&fake, &mut Fed(rx), &mut Collect::all()).await;
    assert!(matches!(result, Err(ProviderError::Unreadable(_))));
    drop(tx);
}

#[tokio::test]
async fn a_frame_that_is_not_the_vocabulary_is_unreadable() {
    let fake = fake(|mut conn| async move {
        conn.hello_and_begin().await;
        let body = br#"{"kind":"nonsense"}"#;
        let mut frame = u32::try_from(body.len()).expect("fits").to_be_bytes().to_vec();
        frame.extend_from_slice(body);
        conn.0.write_all(&frame).await.expect("write");
        let _ = conn.recv().await;
    });
    let (tx, rx) = mpsc::channel(4);
    let result = run(&fake, &mut Fed(rx), &mut Collect::all()).await;
    assert!(matches!(result, Err(ProviderError::Unreadable(_))));
    drop(tx);
}

#[tokio::test]
async fn a_sink_that_stops_gets_the_audio_ended_and_the_done() {
    let partial = TranscriptEvent::Partial {
        text: heard("hel"),
        from: SampleIndex(0),
    };
    let more = partial.clone();
    let fake = fake(|mut conn| async move {
        conn.hello_and_begin().await;
        conn.send(&HostOut::Event(partial)).await;
        assert_eq!(conn.recv().await, Some(HostIn::End));
        // Events after the stop are read and dropped.
        conn.send(&HostOut::Event(more)).await;
        conn.send(&HostOut::Done(end())).await;
    });
    let (tx, rx) = mpsc::channel(4);
    let mut sink = Collect {
        events: Vec::new(),
        limit: Some(1),
    };
    assert_eq!(run(&fake, &mut Fed(rx), &mut sink).await, Ok(end()));
    assert_eq!(sink.events.len(), 1);
    fake.host.await.expect("host script");
    drop(tx);
}

#[tokio::test]
async fn a_dropped_client_cancels_and_closes_the_connection() {
    let (seen_tx, seen_rx) = oneshot::channel();
    let fake = fake(|mut conn| async move {
        conn.hello_and_begin().await;
        assert_eq!(conn.recv().await, Some(HostIn::Audio(chunk(0))));
        seen_tx.send(()).expect("test is waiting");
        // The client future is dropped: a Cancel, then the close.
        assert_eq!(conn.recv().await, Some(HostIn::Cancel));
        assert_eq!(conn.recv().await, None);
    });
    let (tx, rx) = mpsc::channel(4);
    tx.send(chunk(0)).await.expect("send");
    let client = fake.client.clone();
    let task = tokio::spawn(async move {
        let mut sink = Collect::all();
        client
            .transcribe(&request(), &mut Fed(rx), &mut sink)
            .await
    });
    seen_rx.await.expect("host saw the audio");
    task.abort();
    assert!(task.await.expect_err("aborted").is_cancelled());
    fake.host.await.expect("host script");
    drop(tx);
}
