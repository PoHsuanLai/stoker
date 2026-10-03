//! A fake speech server on a Unix socket in a private directory, and the audio helpers the
//! `speech_*` tests share. Each test file uses a subset.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use model_http::{
    AuthHeader, HttpClient, HttpEndpoint, HttpTarget, Proxy, Timeouts, UrlPath, WaitMs,
};
use model_openai_compat::{KOKORO_FORMAT, OpenAiSpeech, SpeechFlavor};
use model_provider::{Flow, ModelName};
use speech_provider::{
    AudioChunk, AudioFormat, AudioPull, AudioSink, AudioSource, Lang, LangChoice, PcmBytes,
    PcmFormat, SampleIndex, SampleRate, SpokenText, SttMode, SttRequest, TranscriptEvent,
    TranscriptSink, TtsRequest, VoiceId,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

// ---- the fake server ------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub head: String,
    pub body: Vec<u8>,
}

/// A reply, written as a chunked body in pieces with a pause between them.
#[derive(Clone)]
pub struct Reply {
    pub status: &'static str,
    pub content_type: &'static str,
    pub retry_after: Option<u32>,
    pub pieces: Vec<Vec<u8>>,
    pub gap_ms: u64,
}

pub fn json(status: &'static str, body: &str) -> Reply {
    Reply {
        status,
        content_type: "application/json",
        retry_after: None,
        pieces: vec![body.as_bytes().to_vec()],
        gap_ms: 0,
    }
}

pub fn pcm(pieces: Vec<Vec<u8>>, gap_ms: u64) -> Reply {
    Reply {
        status: "200 OK",
        content_type: "audio/pcm",
        retry_after: None,
        pieces,
        gap_ms,
    }
}

pub type Handler = Arc<dyn Fn(&Seen) -> Reply + Send + Sync>;

pub struct Fake {
    /// The private directory of the socket, removed when the fake goes.
    dir: PathBuf,
    pub endpoint: HttpEndpoint,
    seen: Arc<Mutex<Vec<Seen>>>,
    pub peer_closed: Arc<AtomicBool>,
}

pub fn endpoint(target: HttpTarget) -> HttpEndpoint {
    HttpEndpoint {
        target,
        proxy: Proxy::Direct,
        base: UrlPath("/v1".into()),
        auth: AuthHeader::None,
        headers: vec![],
        timeouts: Timeouts {
            connect: WaitMs(2_000),
            first_byte: WaitMs(5_000),
            idle: WaitMs(5_000),
        },
    }
}

pub fn scratch() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "speech-provider-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("engine.sock")
}

async fn read_request<S: AsyncReadExt + Unpin>(stream: &mut S) -> Option<Seen> {
    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = stream.read(&mut buf).await.ok().filter(|n| *n > 0)?;
        raw.extend_from_slice(&buf[..n]);
        let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&raw[..end]).into_owned();
        let want = head
            .to_ascii_lowercase()
            .lines()
            .find_map(|l| l.strip_prefix("content-length: ").map(str::to_owned))
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if raw.len() >= end + 4 + want {
            let mut first = head.lines().next().unwrap_or("").split(' ');
            return Some(Seen {
                method: first.next().unwrap_or("").to_owned(),
                path: first.next().unwrap_or("").to_owned(),
                head,
                body: raw[end + 4..end + 4 + want].to_vec(),
            });
        }
    }
}

async fn write_reply<S: AsyncWriteExt + Unpin>(stream: &mut S, reply: &Reply) -> bool {
    let retry = reply
        .retry_after
        .map(|s| format!("Retry-After: {s}\r\n"))
        .unwrap_or_default();
    let head = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\n{retry}Transfer-Encoding: chunked\r\n\r\n",
        reply.status, reply.content_type
    );
    if stream.write_all(head.as_bytes()).await.is_err() {
        return false;
    }
    for piece in &reply.pieces {
        let mut framed = format!("{:x}\r\n", piece.len()).into_bytes();
        framed.extend_from_slice(piece);
        framed.extend_from_slice(b"\r\n");
        if stream.write_all(&framed).await.is_err() || stream.flush().await.is_err() {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(reply.gap_ms)).await;
    }
    stream.write_all(b"0\r\n\r\n").await.is_ok()
}

pub fn fake(handler: impl Fn(&Seen) -> Reply + Send + Sync + 'static) -> Fake {
    let path = scratch();
    let listener = UnixListener::bind(&path).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let peer_closed = Arc::new(AtomicBool::new(false));
    let handler: Handler = Arc::new(handler);
    let (log, closed) = (seen.clone(), peer_closed.clone());
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let (log, closed, handler) = (log.clone(), closed.clone(), handler.clone());
            tokio::spawn(async move {
                let Some(request) = read_request(&mut stream).await else {
                    return;
                };
                let reply = handler(&request);
                log.lock().unwrap().push(request);
                if !write_reply(&mut stream, &reply).await {
                    closed.store(true, Ordering::SeqCst);
                }
            });
        }
    });
    Fake {
        dir: path.parent().map(PathBuf::from).unwrap_or_default(),
        endpoint: endpoint(HttpTarget::Unix(path)),
        seen,
        peer_closed,
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Fake {
    pub fn speech(&self, flavor: SpeechFlavor) -> OpenAiSpeech {
        OpenAiSpeech::new(HttpClient::new(self.endpoint.clone()), flavor)
    }

    pub fn requests(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

// ---- audio ---------------------------------------------------------------------------------

pub const S16_16K: AudioFormat = AudioFormat {
    rate: SampleRate(16_000),
    pcm: PcmFormat::S16Le,
};

pub fn chunk(at: u64, samples: u16) -> AudioChunk {
    let pcm = (0..samples).flat_map(|i| i.to_le_bytes()).collect();
    AudioChunk {
        format: S16_16K,
        at: SampleIndex(at),
        pcm: PcmBytes::new(pcm),
    }
}

pub struct Spoken(pub VecDeque<AudioChunk>);

impl AudioSource for Spoken {
    async fn next(&mut self) -> AudioPull {
        self.0.pop_front().map_or(AudioPull::End, AudioPull::Chunk)
    }
}

#[derive(Default)]
pub struct Events(pub Vec<TranscriptEvent>);

impl TranscriptSink for Events {
    fn event(&mut self, event: TranscriptEvent) -> Flow {
        self.0.push(event);
        Flow::Continue
    }
}

/// Keeps what is played; stops after `stop_after` chunks when told to.
#[derive(Default)]
pub struct Speaker {
    pub chunks: Vec<AudioChunk>,
    pub stop_after: Option<usize>,
}

impl AudioSink for Speaker {
    fn chunk(&mut self, chunk: AudioChunk) -> Flow {
        self.chunks.push(chunk);
        match self.stop_after {
            Some(n) if self.chunks.len() >= n => Flow::Stop,
            _ => Flow::Continue,
        }
    }
}

pub fn stt(lang: LangChoice) -> SttRequest {
    SttRequest {
        model: ModelName("openai/whisper-large-v3".into()),
        mode: SttMode::Batch,
        lang,
        format: S16_16K,
    }
}

pub fn tts() -> TtsRequest {
    TtsRequest {
        model: ModelName("kokoro".into()),
        text: SpokenText::new("Hello there").unwrap(),
        voice: VoiceId::new("af_heart").unwrap(),
        lang: Lang::new("en-US").unwrap(),
        format: KOKORO_FORMAT,
    }
}
