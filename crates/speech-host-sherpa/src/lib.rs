//! A [`Recognizer`] over sherpa-onnx's online (streaming) recognizer with the Nemotron 3.5
//! streaming transducer. CPU only. Audio is held in memory for the utterance; nothing is
//! logged or written.
//!
//! The 560 ms export fixes the model's own chunk; `--chunk-ms` is how much audio the client
//! sends per frame and does not change the engine.

mod pcm;

use model_provider::{ModelName, ProviderError, Support};
use sherpa_onnx::{
    OnlineModelConfig, OnlineRecognizer, OnlineRecognizerConfig, OnlineStream,
    OnlineTransducerModelConfig,
};
use speech_host::{Finished, HostArgs, HostError, Recognizer};
use speech_provider::{
    AudioChunk, AudioMs, HeardText, Lang, LangChoice, LangSet, SampleIndex, SpeechCaps, SpeechIo,
    SpeechModelInfo, SttEnd, SttRequest, TranscriptEvent,
};

/// The catalog id of the one model this build serves.
pub const MODEL: &str = "nemotron-3.5-asr-streaming";
const RATE: u32 = 16_000;
/// Silence fed after the last sample so the final chunk is decoded (0.3 s).
const TAIL_SAMPLES: usize = 4_800;
/// The model card's locale tags, as the catalog entry lists them.
const LANGS: [&str; 40] = [
    "ar-AR", "bg-BG", "cs-CZ", "da-DK", "de-DE", "el-GR", "en-GB", "en-US", "es-ES", "es-US",
    "et-EE", "fi-FI", "fr-CA", "fr-FR", "he-IL", "hi-IN", "hr-HR", "hu-HU", "it-IT", "ja-JP",
    "ko-KR", "lt-LT", "lv-LV", "mt-MT", "nb-NO", "nl-NL", "nn-NO", "pl-PL", "pt-BR", "pt-PT",
    "ro-RO", "ru-RU", "sk-SK", "sl-SI", "sv-SE", "th-TH", "tr-TR", "uk-UA", "vi-VN", "zh-CN",
];

/// One running utterance.
struct Utterance {
    stream: OnlineStream,
    /// Samples fed so far, at 16 kHz.
    heard: u64,
    /// Where the current (unfinished) segment starts.
    segment_from: u64,
    /// The last partial text sent, so an unchanged hypothesis is not repeated.
    last_partial: String,
    /// The stable segments so far.
    finals: Vec<String>,
}

impl std::fmt::Debug for Utterance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Utterance({} samples)", self.heard)
    }
}

pub struct SherpaRecognizer {
    engine: OnlineRecognizer,
    utterance: Option<Utterance>,
}

impl std::fmt::Debug for SherpaRecognizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SherpaRecognizer({:?})", self.utterance)
    }
}

impl SherpaRecognizer {
    /// Loads the int8 encoder, decoder, joiner and `tokens.txt` from `args.model_dir`.
    pub fn load(args: &HostArgs) -> Result<Self, HostError> {
        let file = |name: &str| {
            let path = args.model_dir.join(name);
            path.is_file()
                .then(|| path.to_string_lossy().into_owned())
                .ok_or(HostError::Model)
        };
        let config = OnlineRecognizerConfig {
            model_config: OnlineModelConfig {
                transducer: OnlineTransducerModelConfig {
                    encoder: Some(file("encoder.int8.onnx")?),
                    decoder: Some(file("decoder.int8.onnx")?),
                    joiner: Some(file("joiner.int8.onnx")?),
                },
                tokens: Some(file("tokens.txt")?),
                num_threads: i32::from(args.threads.0),
                provider: Some("cpu".to_string()),
                ..OnlineModelConfig::default()
            },
            decoding_method: Some("greedy_search".to_string()),
            enable_endpoint: true,
            rule1_min_trailing_silence: 2.4,
            rule2_min_trailing_silence: 1.2,
            rule3_min_utterance_length: 20.0,
            ..OnlineRecognizerConfig::default()
        };
        let engine = OnlineRecognizer::create(&config).ok_or(HostError::Model)?;
        Ok(Self {
            engine,
            utterance: None,
        })
    }
}

fn model_info() -> SpeechModelInfo {
    SpeechModelInfo {
        name: ModelName(MODEL.to_string()),
        caps: SpeechCaps {
            streaming: Support::Present,
            partials: Support::Present,
            punctuation: Support::Present,
            timestamps: Support::Present,
            langs: LangSet::Listed(LANGS.iter().filter_map(|t| Lang::new(*t).ok()).collect()),
            max_audio: AudioMs(120_000),
            io: SpeechIo::In {
                input: speech_provider::AudioFormat {
                    rate: speech_provider::SampleRate(RATE),
                    pcm: speech_provider::PcmFormat::S16Le,
                },
            },
        },
    }
}

/// The per-stream language string: `auto`, or the primary subtag of the first preferred tag.
fn language(choice: &LangChoice) -> String {
    match choice {
        LangChoice::Prefer(tags) => tags
            .first()
            .and_then(|t| t.as_str().split('-').next())
            .map_or_else(|| "auto".to_string(), str::to_lowercase),
        LangChoice::Auto => "auto".to_string(),
    }
}

impl Utterance {
    fn ms(&self) -> AudioMs {
        AudioMs(u32::try_from(self.heard * 1000 / u64::from(RATE)).unwrap_or(u32::MAX))
    }
}

impl SherpaRecognizer {
    fn text(&self, utterance: &Utterance) -> String {
        self.engine
            .get_result(&utterance.stream)
            .map(|r| r.text.trim().to_string())
            .unwrap_or_default()
    }

    fn decode(&self, utterance: &Utterance) {
        while self.engine.is_ready(&utterance.stream) {
            self.engine.decode(&utterance.stream);
        }
    }
}

impl Recognizer for SherpaRecognizer {
    fn models(&self) -> Vec<SpeechModelInfo> {
        vec![model_info()]
    }

    fn begin(&mut self, request: &SttRequest) -> Result<(), ProviderError> {
        if request.model.0 != MODEL {
            return Err(ProviderError::BadRequest(
                "the model is not served here".into(),
            ));
        }
        if request.format != model_info_input() {
            return Err(ProviderError::BadRequest(
                "the audio must be 16 kHz mono s16_le".into(),
            ));
        }
        let stream = self.engine.create_stream();
        stream.set_option("language", &language(&request.lang));
        self.utterance = Some(Utterance {
            stream,
            heard: 0,
            segment_from: 0,
            last_partial: String::new(),
            finals: Vec::new(),
        });
        Ok(())
    }

    fn accept(&mut self, chunk: &AudioChunk) -> Result<Vec<TranscriptEvent>, ProviderError> {
        let mut utterance = self.utterance.take().ok_or(ProviderError::NotReady)?;
        let samples = pcm::to_f32(chunk.format.pcm, chunk.pcm.as_slice());
        utterance.stream.accept_waveform(RATE as i32, &samples);
        utterance.heard += samples.len() as u64;
        self.decode(&utterance);
        let text = self.text(&utterance);
        let mut events = Vec::new();
        if self.engine.is_endpoint(&utterance.stream) {
            if !text.is_empty() {
                events.push(TranscriptEvent::Final {
                    text: HeardText(text.clone()),
                    from: SampleIndex(utterance.segment_from),
                    to: SampleIndex(utterance.heard),
                });
                utterance.finals.push(text);
            }
            self.engine.reset(&utterance.stream);
            utterance.segment_from = utterance.heard;
            utterance.last_partial.clear();
        } else if text != utterance.last_partial && !text.is_empty() {
            events.push(TranscriptEvent::Partial {
                text: HeardText(text.clone()),
                from: SampleIndex(utterance.segment_from),
            });
            utterance.last_partial = text;
        }
        self.utterance = Some(utterance);
        Ok(events)
    }

    fn finish(&mut self) -> Result<Finished, ProviderError> {
        let mut utterance = self.utterance.take().ok_or(ProviderError::NotReady)?;
        utterance
            .stream
            .accept_waveform(RATE as i32, &[0.0; TAIL_SAMPLES]);
        utterance.stream.input_finished();
        self.decode(&utterance);
        let text = self.text(&utterance);
        let mut events = Vec::new();
        if !text.is_empty() {
            events.push(TranscriptEvent::Final {
                text: HeardText(text.clone()),
                from: SampleIndex(utterance.segment_from),
                to: SampleIndex(utterance.heard),
            });
            utterance.finals.push(text);
        }
        let end = SttEnd {
            text: HeardText(utterance.finals.join(" ")),
            audio: utterance.ms(),
            served: ModelName(MODEL.to_string()),
        };
        Ok(Finished { events, end })
    }

    fn reset(&mut self) {
        self.utterance = None;
    }
}

fn model_info_input() -> speech_provider::AudioFormat {
    speech_provider::AudioFormat {
        rate: speech_provider::SampleRate(RATE),
        pcm: speech_provider::PcmFormat::S16Le,
    }
}
