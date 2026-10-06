//! The serve loop over a loopback Unix socket pair and a scripted recognizer. No sleeps: the
//! client writes its frames, half-closes, and reads replies to the end of the stream.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use model_provider::{ModelName, ProviderError, Support};
use speech_provider::{
    AudioChunk, AudioFormat, AudioMs, HeardText, HostIn, HostOut, HostVocab, LangChoice, LangSet,
    PcmBytes, PcmFormat, SampleIndex, SampleRate, SpeechCaps, SpeechIo, SpeechModelInfo, SttEnd,
    SttMode, SttRequest, TranscriptEvent, decode_frame, encode_frame,
};

use crate::{Finished, HostArgs, HostError, Recognizer, ThreadCount, serve, serve_connection};

const FORMAT: AudioFormat = AudioFormat {
    rate: SampleRate(16_000),
    pcm: PcmFormat::S16Le,
};

fn name() -> ModelName {
    ModelName("scripted".into())
}

fn info() -> SpeechModelInfo {
    SpeechModelInfo {
        name: name(),
        caps: SpeechCaps {
            streaming: Support::Present,
            partials: Support::Present,
            punctuation: Support::Present,
            timestamps: Support::Absent,
            langs: LangSet::Any,
            max_audio: AudioMs(120_000),
            io: SpeechIo::In { input: FORMAT },
        },
    }
}

fn request() -> SttRequest {
    SttRequest {
        model: name(),
        mode: SttMode::Streaming {
            chunk: AudioMs(560),
        },
        lang: LangChoice::Auto,
        format: FORMAT,
    }
}

fn chunk(at: u64) -> AudioChunk {
    AudioChunk {
        format: FORMAT,
        at: SampleIndex(at),
        pcm: PcmBytes::new(vec![0; 640]),
    }
}

fn text(s: &str) -> HeardText {
    HeardText(s.into())
}

fn partial(s: &str) -> TranscriptEvent {
    TranscriptEvent::Partial {
        text: text(s),
        from: SampleIndex(0),
    }
}

fn end(s: &str) -> SttEnd {
    SttEnd {
        text: text(s),
        audio: AudioMs(40),
        served: name(),
    }
}

/// Plays one list of events per `accept`, then `finish`; records every call.
#[derive(Debug, Default)]
struct Scripted {
    per_accept: VecDeque<Result<Vec<TranscriptEvent>, ProviderError>>,
    begin_error: Option<ProviderError>,
    finish_events: Vec<TranscriptEvent>,
    calls: Vec<&'static str>,
    held: bool,
}

impl Recognizer for Scripted {
    fn models(&self) -> Vec<SpeechModelInfo> {
        vec![info()]
    }

    fn begin(&mut self, _request: &SttRequest) -> Result<(), ProviderError> {
        self.calls.push("begin");
        match self.begin_error.take() {
            Some(e) => Err(e),
            None => {
                self.held = true;
                Ok(())
            }
        }
    }

    fn accept(&mut self, _chunk: &AudioChunk) -> Result<Vec<TranscriptEvent>, ProviderError> {
        self.calls.push("accept");
        self.per_accept.pop_front().unwrap_or(Ok(Vec::new()))
    }

    fn finish(&mut self) -> Result<Finished, ProviderError> {
        self.calls.push("finish");
        Ok(Finished {
            events: self.finish_events.clone(),
            end: end("hello world"),
        })
    }

    fn reset(&mut self) {
        self.calls.push("reset");
        self.held = false;
    }
}

fn hello() -> HostIn {
    HostIn::Hello {
        vocab: HostVocab::CURRENT,
    }
}

fn frames(messages: &[HostIn]) -> Vec<u8> {
    messages
        .iter()
        .flat_map(|m| encode_frame(m).expect("encodes"))
        .collect()
}

fn read_all(bytes: &[u8]) -> Vec<HostOut> {
    let mut out = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let len = u32::from_be_bytes(rest[..4].try_into().expect("header")) as usize;
        out.push(decode_frame(&rest[4..4 + len]).expect("a host reply"));
        rest = &rest[4 + len..];
    }
    out
}

/// Runs `raw` through a connection served on a thread and returns the replies and the recognizer.
fn run_raw(raw: Vec<u8>, rec: Scripted) -> (Vec<HostOut>, Scripted) {
    let (mut client, mut server) = UnixStream::pair().expect("socket pair");
    let host = std::thread::spawn(move || {
        let mut rec = rec;
        let _ = serve_connection(&mut rec, &mut server);
        rec
    });
    client.write_all(&raw).expect("write");
    client
        .shutdown(std::net::Shutdown::Write)
        .expect("half close");
    let mut replies = Vec::new();
    // A host that hangs up on unread input resets the connection; what came first is kept.
    let _ = client.read_to_end(&mut replies);
    let rec = host.join().expect("host thread");
    (read_all(&replies), rec)
}

fn run(messages: &[HostIn], rec: Scripted) -> (Vec<HostOut>, Scripted) {
    run_raw(frames(messages), rec)
}

fn bad_request(out: &HostOut) -> bool {
    matches!(out, HostOut::Failed(ProviderError::BadRequest(_)))
}

#[test]
fn a_whole_utterance() {
    let rec = Scripted {
        per_accept: VecDeque::from([
            Ok(vec![partial("hel")]),
            Ok(vec![]),
            Ok(vec![partial("hello")]),
        ]),
        finish_events: vec![TranscriptEvent::Final {
            text: text("hello world"),
            from: SampleIndex(0),
            to: SampleIndex(640),
        }],
        ..Scripted::default()
    };
    let msgs = [
        hello(),
        HostIn::Begin(request()),
        HostIn::Audio(chunk(0)),
        HostIn::Audio(chunk(320)),
        HostIn::Audio(chunk(640)),
        HostIn::End,
    ];
    let (out, rec) = run(&msgs, rec);
    assert_eq!(
        out,
        vec![
            HostOut::Hello {
                vocab: HostVocab::CURRENT,
                models: vec![info()]
            },
            HostOut::Event(partial("hel")),
            HostOut::Event(partial("hello")),
            HostOut::Event(TranscriptEvent::Final {
                text: text("hello world"),
                from: SampleIndex(0),
                to: SampleIndex(640)
            }),
            HostOut::Done(end("hello world")),
        ]
    );
    // The utterance is dropped after Done, and again when the connection closes.
    assert_eq!(
        rec.calls,
        [
            "begin", "accept", "accept", "accept", "finish", "reset", "reset"
        ]
    );
    assert!(!rec.held);
}

#[test]
fn a_second_utterance_after_done_on_the_same_connection() {
    let msgs = [
        hello(),
        HostIn::Begin(request()),
        HostIn::End,
        HostIn::Begin(request()),
        HostIn::End,
    ];
    let (out, _) = run(&msgs, Scripted::default());
    let dones = out.iter().filter(|o| matches!(o, HostOut::Done(_))).count();
    assert_eq!(dones, 2);
}

#[test]
fn nothing_but_hello_is_accepted_first() {
    let msgs = [
        HostIn::Begin(request()),
        HostIn::Audio(chunk(0)),
        HostIn::End,
        HostIn::Cancel,
    ];
    let (out, rec) = run(&msgs, Scripted::default());
    assert_eq!(out.len(), 4, "every message before hello is refused");
    assert!(out.iter().all(bad_request));
    assert!(!rec.calls.contains(&"begin"));
}

#[test]
fn a_wrong_vocab_is_refused_and_hello_can_be_retried() {
    let msgs = [
        HostIn::Hello {
            vocab: HostVocab(99),
        },
        hello(),
        hello(),
    ];
    let (out, _) = run(&msgs, Scripted::default());
    assert!(bad_request(&out[0]));
    assert!(matches!(out[1], HostOut::Hello { .. }));
    assert!(bad_request(&out[2]), "a second hello is refused");
}

#[test]
fn a_second_begin_is_refused_and_the_running_utterance_goes_on() {
    let rec = Scripted {
        per_accept: VecDeque::from([Ok(vec![partial("a")])]),
        ..Scripted::default()
    };
    let msgs = [
        hello(),
        HostIn::Begin(request()),
        HostIn::Begin(request()),
        HostIn::Audio(chunk(0)),
        HostIn::End,
    ];
    let (out, rec) = run(&msgs, rec);
    assert!(bad_request(&out[1]));
    assert_eq!(out[2], HostOut::Event(partial("a")));
    assert!(matches!(out[3], HostOut::Done(_)));
    assert_eq!(rec.calls.iter().filter(|c| **c == "begin").count(), 1);
}

#[test]
fn cancel_drops_the_utterance_without_a_reply() {
    let msgs = [
        hello(),
        HostIn::Begin(request()),
        HostIn::Audio(chunk(0)),
        HostIn::Cancel,
        HostIn::Audio(chunk(0)),
        HostIn::Begin(request()),
        HostIn::End,
    ];
    let (out, rec) = run(&msgs, Scripted::default());
    assert!(matches!(out[0], HostOut::Hello { .. }));
    assert!(bad_request(&out[1]), "audio after Cancel has no utterance");
    assert!(matches!(out[2], HostOut::Done(_)));
    assert_eq!(out.len(), 3);
    assert_eq!(rec.calls.iter().filter(|c| **c == "finish").count(), 1);
}

#[test]
fn audio_or_end_with_no_utterance_is_refused() {
    let msgs = [hello(), HostIn::Audio(chunk(0)), HostIn::End];
    let (out, _) = run(&msgs, Scripted::default());
    assert!(bad_request(&out[1]) && bad_request(&out[2]));
}

#[test]
fn a_begin_the_engine_refuses_fails_and_the_next_begin_works() {
    let rec = Scripted {
        begin_error: Some(ProviderError::BadRequest("unserved model".into())),
        ..Scripted::default()
    };
    let msgs = [
        hello(),
        HostIn::Begin(request()),
        HostIn::Begin(request()),
        HostIn::End,
    ];
    let (out, _) = run(&msgs, rec);
    assert_eq!(
        out[1],
        HostOut::Failed(ProviderError::BadRequest("unserved model".into()))
    );
    assert!(matches!(out[2], HostOut::Done(_)));
}

#[test]
fn an_engine_error_mid_utterance_ends_it() {
    let rec = Scripted {
        per_accept: VecDeque::from([Err(ProviderError::NotReady)]),
        ..Scripted::default()
    };
    let msgs = [
        hello(),
        HostIn::Begin(request()),
        HostIn::Audio(chunk(0)),
        HostIn::End,
    ];
    let (out, rec) = run(&msgs, rec);
    assert_eq!(out[1], HostOut::Failed(ProviderError::NotReady));
    assert!(bad_request(&out[2]), "End after a failure has no utterance");
    assert!(!rec.held);
}

#[test]
fn a_connection_closed_mid_utterance_drops_it() {
    let msgs = [hello(), HostIn::Begin(request()), HostIn::Audio(chunk(0))];
    let (_, rec) = run(&msgs, Scripted::default());
    assert_eq!(rec.calls.last(), Some(&"reset"));
    assert!(!rec.held);
}

#[test]
fn a_frame_that_is_not_a_host_message_is_refused_and_the_connection_goes_on() {
    let mut raw = frames(&[hello()]);
    let junk = br#"{"kind":"nonsense"}"#;
    raw.extend_from_slice(&(junk.len() as u32).to_be_bytes());
    raw.extend_from_slice(junk);
    raw.extend(frames(&[HostIn::Begin(request()), HostIn::End]));
    let (out, _) = run_raw(raw, Scripted::default());
    assert!(bad_request(&out[1]));
    assert!(matches!(out[2], HostOut::Done(_)));
}

#[test]
fn an_oversize_frame_ends_the_connection_before_its_body_is_read() {
    let mut raw = frames(&[hello()]);
    raw.extend_from_slice(&((1u32 << 20) + 1).to_be_bytes());
    // Messages after the bad header are never served.
    raw.extend(frames(&[HostIn::Begin(request()), HostIn::End]));
    let (out, rec) = run_raw(raw, Scripted::default());
    assert_eq!(out.len(), 1);
    assert!(!rec.calls.contains(&"begin"));
}

#[test]
fn the_socket_is_owner_only_and_a_stale_one_is_replaced() {
    let dir = scratch("bind");
    let path = dir.join("host.sock");
    let first = crate::bind(&path).expect("binds");
    let mode = std::fs::metadata(&path).expect("stat").permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    drop(first);
    // The file is left behind by the dropped listener: a stale socket.
    let second = crate::bind(&path).expect("a stale socket is replaced");

    let host = std::thread::spawn(move || {
        let (mut stream, _) = second.accept().expect("accept");
        let mut rec = Scripted::default();
        serve_connection(&mut rec, &mut stream).expect("served");
    });
    let mut client = UnixStream::connect(&path).expect("connect");
    client
        .write_all(&frames(&[hello(), HostIn::Begin(request()), HostIn::End]))
        .expect("write");
    client
        .shutdown(std::net::Shutdown::Write)
        .expect("half close");
    let mut replies = Vec::new();
    client.read_to_end(&mut replies).expect("read");
    host.join().expect("host thread");
    assert!(matches!(read_all(&replies).last(), Some(HostOut::Done(_))));
    std::fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn a_regular_file_at_the_socket_path_is_never_removed() {
    let dir = scratch("file");
    let path = dir.join("precious");
    std::fs::write(&path, b"mine").expect("write");
    assert_eq!(crate::bind(&path).err(), Some(HostError::Bind));
    assert_eq!(std::fs::read(&path).expect("read"), b"mine");
    std::fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn serve_without_an_engine_says_model() {
    let args = HostArgs {
        socket: "/nonexistent/s.sock".into(),
        model_dir: "/nonexistent".into(),
        threads: ThreadCount(1),
        chunk: AudioMs(560),
    };
    assert_eq!(serve(&args), Err(HostError::Model));
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("speech-host-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}
