# Architecture

stoker is the model layer of the desktop program: one trait for chat, tool-calling, vision and
computer-use models; the backends that speak to local engines; the computer-use action
vocabulary, parsers and per-step session; image preparation; the curated model catalog; and the
supervisor that starts and stops the engines; and speech: the speech-to-text and text-to-speech
seams, voice-activity detection, the host process that runs the STT engine, and its client. It is portable and has no dependency on porter:
porter's `inferd` is where stoker types and porter types meet (`inferd::bridge`). The design
is the companion agent spec (`SPEC.md`, `models.md` in the spec worktree); this file is the map of
the code that freezes its interfaces. `CONVENTIONS.md` holds the rules; `FINDINGS.md` the open
items.

Reading order: section 1 (find the crate), section 3 (find the home), section 4 (find the
trait), section 6 (copy the recipe).

## 1. Crates and allowed edges

| Crate | Purpose | I/O |
| --- | --- | --- |
| `cua-action` | the provider-neutral action vocabulary: coordinate spaces (`CoordSpace`, `WindowSpace`, `ImageSpace`, `GridSpace`), `Point`/`Size`/`Rect`/`Length`, `Target`, `CuaAction<S>`, `ActionClass`, keys and chords, bounded text, dialect names (`CuaDialect`, `ModelSpace`) | none |
| `vision-prep` | `ResizeRule`, `fit`, `image_tokens`, `FrameMap`, `RawFrame`, `prepare` (feature `pixels`) | none; `pixels` is CPU only |
| `model-provider` | `TurnRequest`, `Message`, `Part`, `ToolSpec`, `TurnEvent`, `TurnSink`, `Caps`, the `Provider` trait, `ProviderError`; the controls (`Sampling`, `ToolParallelism`, `EngineExtras`, `ThoughtSeal`, `Knob`); `embed` (`Embedder`, `EmbedTurn`, `EmbedRole`, `plan_batches`); `shape` (`Shape`, `Extract`, the schema, GBNF and regex conversions); `retry` (`RetryClass`, `next_wait`, `Retrying`); `sequence` (`check`); feature `testing`: `ScriptedProvider` | none |
| `cua-parse` | `parse_text`, `parse_tool_calls`, `Parsed`, `Dropped`, `ParseLimits` | none |
| `cua-vendors` | the `WireCodec` trait and one codec per `WireDialect` (formerly `cua-wire`): tool declarations, decoders and result encoders for the Anthropic, OpenAI and Gemini computer-use tools | none |
| `cua-session` | `CuaSession`: history window, prompt assembly (`prompts/`), parse with one repair, mapping to window space; `TurnSettings`, `TranscriptSink` | none |
| `model-replay` | cassette format, `ReplayProvider`, `RecordingProvider` over a `CassetteSink`; `wire`: wire cassettes recorded and replayed at the `Transport` seam (`RecordingTransport`, `ReplayTransport`, `ChunkPlan`); `check_sequence`; `speech`: speech cassettes (audio as digests), `SpeechReplay`, `RecordingSpeech` | an injected sink |
| `model-catalog` | `ModelEntry` (chat caps and `SamplingDefaults`, or the `speech` table, or the `embed` table), `EngineProfile`, `parse_entry`, `merge_catalogs`; the shipped `catalog/*.toml` | none |
| `engine-supervisor` | the pure `step` and `budget`; `command` (catalog entry to `UnitSpec`); the seams `EngineHost`, `ReadyProbe`, `GpuProbe`; feature `testing`: fakes | none; the daemon fills the seams |
| `model-http` | `HttpEndpoint` (with `Timeouts` and `ExtraHeader`s), `HttpTarget` (Tcp, Unix, Tls), `AuthHeader`, the pure `SseDecoder` and `NdjsonDecoder`, `ResponseHead`, `BodySink` (head, then chunks), `Exchange`, the `Transport` seam, `Upload` and `UploadTransport` (a POST of bytes), and `HttpClient: Transport + UploadTransport` (hyper over TCP and Unix sockets, behind the `hyper` feature) | yes (the transport) |
| `model-wire` | the wire half of an endpoint: `ChatCodec`, `ChatDecoder`, `EmbedCodec`, `ErrorWire`, `CodecError`, and `Driver<C, T>`, the `Provider` (and `Embedder`) made of a codec and a transport | none (pure over the `Transport` trait) |
| `model-extract` | structured extraction as a pure machine: `choose` (native constraint, synthetic tool call or prompted), `ExtractSession`, `Extracted`, `ExtractFailure` | none |
| `genai-names` | the OpenTelemetry GenAI attribute, metric, operation and finish-reason names as constants; zero dependencies; no content attribute exists | none |
| `model-openai-compat` | the chat-completions codec `OpenAiCodec` (pure `encode_request`, `StreamDecoder`), `Flavor::quirks` (the table of what differs between servers), `OpenAiCompat = Driver<OpenAiCodec, HttpClient>`; `audio`: `encode_speech_request`, `encode_transcription`, `PcmDecoder`, and `OpenAiSpeech` (both speech traits) | none of its own: the HTTP is `model-http`'s `Transport` (the speech provider is `OpenAiSpeech<T: UploadTransport>`, an `HttpClient` by default) |
| `speech-provider` | audio and text types, `SpeechToText`, `TextToSpeech`, `AudioSource`/`AudioSink`, `VoiceActivity`, `SpeechCaps`, the host wire and its framing; feature `testing`: `ScriptedStt`, `ScriptedTts`, `ScriptedVad` | none |
| `speech-vad` | `EnergyGate`, `Framer`, `level_of`, the `endpoint` machine | none |
| `speech-host-client` | `SpeechHostClient: SpeechToText` over the host's Unix socket | yes (the socket, when filled) |
| `speech-vad-silero` | `SileroVad: VoiceActivity` over `ort`; **excluded from the workspace** | the runtime |
| `speech-host` | the STT engine binary: sherpa-onnx (built without TTS) behind a Unix socket; **excluded from the workspace**, runs only as a confined engine unit | the runtime and the socket |

Allowed direct edges (checked by `scripts/check-boundary.sh`; dev-dependencies are outside it):

| Crate | May depend on |
| --- | --- |
| `cua-action`, `model-http`, `genai-names` | nothing of ours |
| `vision-prep` | `cua-action` |
| `model-provider` | `cua-action`, `genai-names`, `vision-prep` |
| `model-wire` | `model-http`, `model-provider` |
| `model-extract` | `model-provider` |
| `cua-parse` | `cua-action`, `model-provider` |
| `cua-vendors` | `cua-action`, `cua-parse`, `model-provider` |
| `cua-session` | `cua-action`, `cua-parse`, `cua-vendors`, `model-provider`, `vision-prep` |
| `model-replay` | `model-http`, `model-provider`, `speech-provider`, `vision-prep` |
| `model-catalog` | `model-provider`, `speech-provider` |
| `engine-supervisor` | `model-catalog` |
| `model-openai-compat` | `model-http`, `model-provider`, `model-wire`, `speech-provider` |
| `speech-provider` | `model-provider` |
| `speech-vad` | `speech-provider` |
| `speech-host-client` | `model-provider`, `speech-provider` |
| `speech-vad-silero` (excluded) | `speech-provider`, `speech-vad` |
| `speech-host` (excluded) | `speech-provider` |

External boundaries: the pure crates (everything but `model-http`, the one io crate) never reach `tokio`,
`hyper`, `hyper-util`, `rustls`, `zbus`, `zvariant`, `reqwest`, `ureq`, `wayland-client`,
`wayland-backend`, `reis`, `atspi`, `oo7`, `ort`, `fastembed`, `rusqlite`, `notify`,
`cedar-policy` or `rmcp` (default features); `vision-prep` reaches `image` and `fast_image_resize`
only through `pixels`. `model-wire`, `model-extract` and `model-openai-compat` are pure over the `Transport` trait: they
reach no HTTP stack and no runtime: `HttpClient`'s hyper transport sits behind `model-http`'s
`hyper` feature, which only a daemon (or a loopback test) turns on, and their rows keep checking
default features. `model-http`
never reaches the second half of that list (a bus, a compositor, a database, an inference runtime). No crate reaches a `porter-*` crate. No speech
crate reaches an audio device crate (`pipewire`, `libpulse-binding`, `libpulse-simple-binding`,
`cpal`: capture and playback belong to docket's `voiced`) or a TTS stack with a GPL grapheme
step (`espeak-rs`, `espeak-ng`, `espeak-ng-sys`, `piper-rs`): text to speech is a separate engine
process (Kokoro-FastAPI), and `speech-host` links sherpa-onnx built without TTS. The two
excluded crates are not workspace members; `check-boundary.sh` checks them from their own
manifests (their direct edges and the same lists), and they alone may reach `ort` and `sherpa-onnx`.

Downstream, porter's `inferd` takes `cua-action`, `vision-prep`, `model-provider`,
`cua-session`, `model-catalog`, `engine-supervisor`, `model-http`, `model-wire`,
`model-openai-compat`, `model-extract`, `speech-provider` and `speech-host-client` (SPEC.md 1.3, voice.md 2.2); docket's `voiced` takes
`speech-vad` and `speech-vad-silero`; porter-infer takes `cua-action` only. cuad, almanac, docket and sill do not name
stoker.

## 2. Modules

| Crate | Modules |
| --- | --- |
| `cua-action` | `space` < `geometry`, `target`, `text`, `keys` < `dialect`, `action` |
| `vision-prep` | `rule` < `frame_map` < `pixels` |
| `model-provider` | `units`, `ids` < `control`, `request` < `caps`, `event` < `provider` < `embed`, `shape` (`check`, `gbnf`, `pattern`, `schema`, `from_schema`), `sequence` < `retry` < `scripted` (feature `testing`) |
| `cua-parse` | `limits` < `outcome` < `common`, `scan`, `chord` < `ui_tars`, `tools` < `parse` |
| `cua-vendors` | `step_result`, `args`, `safety`, `results` < `anthropic`, `openai`, `gemini` < `codec` |
| `cua-session` | `model`, `history`, `prompt`, `reply`, `window`, `transcript` < `session` |
| `model-replay` | `print` < `cassette` < `sequence` < `provider`, `speech`, `wire` |
| `model-catalog` | `engine`, `entry` < `parse` |
| `engine-supervisor` | `state` < `unit`, `budget` < `step` < `host` < `fakes` (feature `testing`) |
| `model-http` | `target`, `auth`, `head` < `sse`, `ndjson` < `client` < `exchange` < `hyper_client` (feature `hyper`) |
| `model-wire` | `codec` < `driver` |
| `model-extract` | `mode` < `session` < `shaped` |
| `genai-names` | `lib` |
| `model-openai-compat` | `quirks`, `codec` < `provider`; `audio` (`codec` < `provider`) |
| `speech-provider` | `audio`, `text` < `vad`, `stt`, `tts`, `caps` < `host_wire` < `testing` (feature `testing`) |
| `speech-vad` | `level` < `energy`, `framer` < `endpoint` |
| `speech-host-client` | `lib` |
| `speech-vad-silero`, `speech-host` | `lib` (and `main` for the host) |

## 3. One home per concept

| Concept | Home |
| --- | --- |
| coordinate spaces and typed points | `cua-action::space`, `geometry` |
| the action enum, its class, its space mapping | `cua-action::action` (`CuaAction::class`, `map_points`) |
| limits on model-supplied text, numbers | `cua-action::text`, `keys` (checked at construction and on deserialisation) |
| dialect names | `cua-action::dialect` |
| device size, scale, pixel format | `cua-action::geometry` |
| resize rules, token estimate | `vision-prep::rule` (`fit` mirrors Qwen's `smart_resize`) |
| window, image and grid coordinate mapping | `vision-prep::FrameMap` (the only place; refuses outside the frame, never clamps) |
| encoded images | `vision-prep::pixels` (`prepare`: resize once, encode once) |
| the request, event and error vocabulary of one turn | `model-provider` |
| what a model can do | `model-provider::Caps` |
| action parsing | `cua-parse` (the only parser of model text) |
| vendor computer-use encodings | `cua-vendors` |
| prompt assembly, repair, history for a step | `cua-session` |
| cassette format and request fingerprints | `model-replay` |
| the model table and its file format | `model-catalog` |
| GPU memory in `MiB` | `model-catalog::MiB` (the supervisor uses it) |
| turning a catalog entry into a unit to run | `engine-supervisor::command` |
| engine lifecycle and the VRAM budget | `engine-supervisor::step`, `budget` (the only place) |
| the SSE framing | `model-http::SseDecoder` |
| the NDJSON framing | `model-http::NdjsonDecoder` |
| a response's status and the few headers a codec reads | `model-http::ResponseHead` (delivered by `BodySink::head` before any chunk) |
| delivery of one exchange (TCP, Unix, TLS, in process, record, replay) | `model-http::Transport` |
| a wire's request, reply and error format | `model-wire::{ChatCodec, EmbedCodec}`; the OpenAI-compatible one is `model-openai-compat::OpenAiCodec` |
| what differs between OpenAI-compatible servers | `model-openai-compat::Flavor::quirks` |
| sampling, tool parallelism, per-flavor engine knobs | `model-provider::control` (no untyped parameter bag) |
| embeddings through a provider, roles, batch plan | `model-provider::embed` |
| the vocabulary of structured outputs and its conversions | `model-provider::shape` |
| how to ask for a typed output and repair it | `model-extract` (inferd runs it) |
| retry classes and backoff | `model-provider::retry` (inferd runs it) |
| the order a conversation must be in | `model-provider::sequence` |
| OpenTelemetry GenAI names | `genai-names` (our own `quire.*` names are porter's `prov::trace`) |
| wire cassettes (HTTP exchanges at the `Transport`) | `model-replay::wire` |
| default sampling of a model | `model-catalog::SamplingDefaults` |
| audio formats, positions and durations; the bytes of PCM | `speech-provider::audio` (`AudioFormat`, `SampleIndex`, `AudioMs`, `PcmBytes`) |
| recognised and spoken text, languages and voices | `speech-provider::text` (`HeardText`, `SpokenText`, `Lang`, `VoiceId`) |
| what a speech model can do | `speech-provider::SpeechCaps` |
| the inferd to `speech-host` wire and its framing | `speech-provider::host_wire` |
| voice-activity detection, loudness, endpointing | `speech-vad` (`EnergyGate`, `level_of`, `endpoint`); the Silero model in `speech-vad-silero` |
| the speech request wire of OpenAI-compatible servers | `model-openai-compat::audio` |
| speech cassettes | `model-replay::speech` |
| the chat-completions wire | `model-openai-compat::codec` |

Names SPEC.md 2 settled for this repo: `CoordSpace` (was `Space`), `ToolCallId` (was `CallId`),
`TurnUsage` (was `Usage`), `Role` stays `model-provider`'s, `Repeat` stays `cua-action`'s, and the
catalog's `roles` are `CatalogKind`s (stoker's copy of porter's `AiKind`, same slugs).

## 4. Traits (the seams) and closed enums

```rust
// model-provider: one per backend (OpenAiCompat, and the cloud backends when they exist), plus
// ScriptedProvider, ReplayProvider and RecordingProvider.
pub trait Provider: Send + Sync {
    fn describe(&self) -> impl Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send;
    fn turn<K: TurnSink>(&self, request: &TurnRequest, sink: &mut K)
        -> impl Future<Output = Result<TurnEnd, ProviderError>> + Send;
}
pub trait TurnSink: Send { fn event(&mut self, event: TurnEvent) -> Flow; }

// cua-vendors: one per vendor, as the closed enum WireCodecs.
pub trait WireCodec {
    fn tools(&self, image: Size<ImageSpace>) -> Vec<ToolSpec>;
    fn decode(&self, calls: &[ToolCall], safety: &[SafetySignal]) -> Result<Parsed, WireError>;
    fn results(&self, done: &[StepResult], next: &ImageInput) -> Vec<Part>;
}

// engine-supervisor: systemd transient units, child processes, the fakes.
pub trait EngineHost: Send + Sync {
    fn spawn(&self, id: &EngineId, unit: &UnitSpec) -> impl Future<Output = Result<(), HostError>> + Send;
    fn stop(&self, id: &EngineId) -> impl Future<Output = Result<(), HostError>> + Send;
    fn exited(&self, id: &EngineId) -> impl Future<Output = ExitCode> + Send;
}
pub trait ReadyProbe: Send + Sync { fn probe(&self, id: &EngineId) -> impl Future<Output = Probe> + Send; }
pub trait GpuProbe: Send + Sync { fn memory(&self) -> impl Future<Output = Result<GpuMemory, GpuError>> + Send; }

// speech-provider: one per backend (SpeechHostClient, OpenAiSpeech, SpeechReplay, RecordingSpeech);
// `SttBackend` and `TtsBackend` are closed enums in porter's inferd. Cancellation is drop.
pub trait SpeechToText: Send + Sync {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send;
    fn transcribe<A: AudioSource, K: TranscriptSink>(&self, request: &SttRequest, audio: &mut A, sink: &mut K)
        -> impl Future<Output = Result<SttEnd, ProviderError>> + Send;
}
pub trait TextToSpeech: Send + Sync {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send;
    fn speak<K: AudioSink>(&self, request: &TtsRequest, sink: &mut K)
        -> impl Future<Output = Result<TtsEnd, ProviderError>> + Send;
}
pub trait AudioSource: Send { fn next(&mut self) -> impl Future<Output = AudioPull> + Send; }
pub trait TranscriptSink: Send { fn event(&mut self, event: TranscriptEvent) -> Flow; }
pub trait AudioSink: Send { fn chunk(&mut self, chunk: AudioChunk) -> Flow; }   // Flow::Stop = barge-in
// synchronous: CPU only, one 32 ms frame, no I/O (EnergyGate, SileroVad, ScriptedVad)
pub trait VoiceActivity: Send { fn push(&mut self, frame: &Frame512) -> (Voiced, SpeechProb); fn reset(&mut self); }

// model-replay: where recorded interactions go (a file in a dev script, a Vec in a test).
pub trait CassetteSink: Send + Sync { fn write(&self, line: &Interaction) -> Result<(), SinkError>; }
pub trait SpeechCassetteSink: Send + Sync { fn write(&self, line: &SpeechInteraction) -> Result<(), SinkError>; }

// model-http: where a response goes as it arrives (the head first), and what delivers an exchange.
pub trait BodySink: Send {
    fn head(&mut self, head: &ResponseHead) -> ChunkFlow;
    fn chunk(&mut self, bytes: &[u8]) -> ChunkFlow;
}
pub trait Transport: Send + Sync {
    fn exchange<K: BodySink>(&self, ex: &Exchange, sink: &mut K)
        -> impl Future<Output = Result<HttpStatus, HttpError>> + Send;
}
// A POST of bytes (a multipart upload); `HttpClient` implements both.
pub trait UploadTransport: Transport {
    fn upload<K: BodySink>(&self, up: &Upload, sink: &mut K)
        -> impl Future<Output = Result<HttpStatus, HttpError>> + Send;
}

// model-wire: the wire of an endpoint, apart from how bytes travel. `Driver<C, T>` is the
// Provider; inferd keeps a closed enum of the instantiations.
pub trait ErrorWire { fn classify(&self, head: &ResponseHead, body: &[u8]) -> ProviderError; }
pub trait ChatCodec: ErrorWire + Send + Sync {
    type Decoder: ChatDecoder + Send;
    fn encode(&self, request: &TurnRequest) -> Result<Exchange, CodecError>;
    fn decoder(&self, served: ModelName) -> Self::Decoder;
    fn describe(&self) -> Exchange;
    fn parse_models(&self, body: &[u8]) -> Result<Vec<ModelInfo>, CodecError>;
}
pub trait ChatDecoder {
    fn feed(&mut self, frame: &str) -> Result<Vec<TurnEvent>, CodecError>;
    fn finish(self) -> Result<TurnEnd, CodecError>;
}
pub trait EmbedCodec: ErrorWire + Send + Sync {
    fn encode_embed(&self, turn: &EmbedTurn) -> Result<Exchange, CodecError>;
    fn decode_embed(&self, served: ModelName, body: &[u8]) -> Result<EmbedEnd, CodecError>;
}

// model-provider: an embedding backend; where a retry waits; a typed output.
pub trait Embedder: Send + Sync {
    fn embed(&self, turn: &EmbedTurn) -> impl Future<Output = Result<EmbedEnd, ProviderError>> + Send;
}
pub trait Sleeper: Send + Sync { fn sleep(&self, wait: WaitMs) -> impl Future<Output = ()> + Send; }
pub trait Extract: Sized { fn shape() -> Shape; fn read(json: &JsonText) -> Result<Self, ShapeFault>; }

// model-replay: where recorded wire exchanges go.
pub trait WireSink: Send + Sync { fn write(&self, exchange: &WireExchange) -> Result<(), SinkError>; }
```

Closed sets stay enums: `CuaAction`, `Target`, `CuaDialect` (`WireDialect`, `TextDialect`,
`ToolDialect`), `ModelSpace`, `ResizeRule`, `Part`, `ToolSpec`, `TurnEvent`, `ProviderError`,
`EngineState`, `SupervisorIn`, `SupervisorOut`, `HttpTarget`, `AuthHeader`, `Flavor`,
`WireCodecs`, `WeightFiles`, `Licence`, `EngineKind`, `CatalogKind`, `PcmFormat`, `SttMode`,
`TranscriptEvent`, `LangChoice`, `LangSet`, `SpeechIo`, `HostIn`, `HostOut`, `Voiced`, `Endpoint`,
`SpeechFlavor`, `SpeechInteraction`, `Framing`, `Verb`, `BodyKind`, `RouteRoot`, `Knob`, `EngineExtras`, `ThoughtSeal`, `ToolParallelism`, `Shape`, `ExtractMode`, `RetryClass`, `WireBody`, `WireEnd`, `ChunkPlan`, `UsageAsk`, `ToolNaming`, `ToolImages`, `DimensionsField`, `ShapeWithTools`.

## 5. What is frozen, what is built, what is stubbed

Frozen means: the public types, trait signatures, serde forms (adjacently tagged `kind`/`v`),
the catalog file format and the cassette file format below are the interface other repos build
on; a change is an edit of SPEC.md first. Every `todo!()` is listed in `FINDINGS.md`.

| Piece | State |
| --- | --- |
| `cua-action`: every type, `class`, `map_points`, the text and number limits | built, tested (round trips, pinned JSON, tables, compile-fail) |
| `vision-prep`: types and serde | built, tested (`fit` against the reference `smart_resize`, the map tables and proptest, `prepare` with the `pixels` feature) |
| `model-provider`: every type, `JsonText`, `ToolName`, `ImageBytes`, `ScriptedProvider` | built, tested |
| `cua-parse`: types and limits | built, tested (UiTars15, QwenComputerUse and a provisional Holo31; `never_panics` proptest; nightly fuzz targets in `crates/cua-parse/fuzz`) |
| `cua-vendors`: trait, enum, `StepResult`, the four codecs | built, tested (doc examples, stop-at-first-failure, safety only adds, a total-decoding proptest); no cloud backend calls them yet |
| `cua-session`: types, `begin`, `request`, `absorb`, `absorb_for` (both `&mut self`, in place) | built, tested (history window, one repair, mapping, vendor wire history, wire-cassette runs of Holo, Qwen and UI-TARS); the Holo and Qwen schemas are ours until a recorded step |
| `model-replay`: cassette format (engine stamp, context sizes, speech caps, interaction id and hash, `Strict` mode), round trip; wire cassette format, `RecordingTransport` and `ReplayTransport` | built, tested (SSE and NDJSON recordings under every chunking); the providers, `RequestPrint::{of, hash}`, `check_sequence`, `Cassette::check_sequences` per their F1 state |
| `model-catalog`: types, `parse_entry`, `merge_catalogs`, `VramEstimate::need`, `gpu_need`, `SamplingDefaults`, `holo-3.1-4b`, the five speech entries | built, tested |
| `engine-supervisor`: types, `Supervisor::new`, config defaults, fakes, `step`, `budget`, `command` | built, tested (the lifecycle table, the budget table with the in-turn window, `command` for vLLM, llama-server, the speech host and Kokoro) |
| `model-http`: types, `ResponseHead`, `BodySink::head`, `Exchange`, `Transport`, `Timeouts`, `ExtraHeader`, `HttpError`, the SSE and NDJSON decoders, `Transport for HttpClient` (feature `hyper`) | built; round-trip, pinned-JSON, proptest and loopback-socket tested (TLS and the egress proxy answer `HttpError::Tls` and `Connect` until the first cloud backend) |
| `model-wire`: `ChatCodec`, `ChatDecoder`, `EmbedCodec`, `ErrorWire`, `CodecError`, `Driver` | built, tested with a scripted transport and a line codec (head decides, framer, decoder fault, Retry-After, stop, every transport failure, embed checks) |
| `model-extract`: `ExtractMode`, `ToolsPresent`, `ExtractSession`, `Extracted`, `ExtractFailure`, `choose`, `request`, `absorb` | built, tested |
| `genai-names`: every name, `Operation`, `Finish` | built, tested (names are values, so there is no stub) |
| `model-provider` amendment: `Sampling`, `EngineExtras`, `Knob`, `ThoughtSeal`, the `embed`, `shape`, `retry` and `sequence` types, `OutputShape::{Gbnf, Choice}`, `Constraint::{Gbnf, Choice}`, `ProviderError::Server`, `TurnUsage.cached`, `ModelInfo.{loaded_context, trained_context}` | built, round-trip and pinned-JSON tested; `plan_batches`, `EmbedEnd::check`, the `Shape` conversions and checker, `retry_class`, `next_wait`, `Retrying`, `sequence::check` stubbed |
| `model-openai-compat`: types, `Flavor::quirks`, `OpenAiCodec` (`encode_request`, `encode`, `classify`, `describe`, `parse_models`, `encode_embed`, `decode_embed`), `StreamDecoder` | built, tested (golden bodies, the flavor table, error classes, wire cassettes under every chunking, one loopback-socket run of the whole stack); the `audio` codec (`encode_speech_request`, `encode_transcription`, `decode_transcription`, `PcmDecoder`) is built and tested; `OpenAiSpeech` is stubbed (it needs a multipart POST) |
| `speech-provider`: every type, the checked names, `AudioChunk::{samples, duration}`, the host framing, the `testing` fakes | built, tested (round trips, pinned JSON, redaction, duration table, the fakes' scripts) |
| `speech-vad`: types, defaults, `level_of`, `EnergyGate`, `Framer::push`, `endpoint` | built, tested (level rows, gate table, framer proptest, endpoint table, the chain over synthetic audio) |
| `speech-host-client`: types | built; both trait bodies stubbed |
| `speech-vad-silero`, `speech-host` | skeletons (excluded from the workspace); every body stubbed |
| `model-replay::speech`: cassette format, round trip | built, tested; the prints, `SpeechReplay` and `RecordingSpeech` stubbed |

## 6. Recipes

**Add a backend** (a `Provider`): a crate `model-<name>` depending on `model-wire`, `model-http` and
`model-provider`; a pure codec (`ChatCodec` and its `ChatDecoder`, `ErrorWire::classify`, and
`EmbedCodec` if it embeds) tested against wire fixtures in `fixtures/<name>/` recorded at the
`Transport` by a dev script; the provider is `Driver<YourCodec, HttpClient>`; its row in
`scripts/check-boundary.sh` (RULES and EDGES) and in section 1; its variant in porter's
`inferd::AdapterModel`.

**Add a dialect**: the variant in `cua-action::dialect` (`TextDialect`, `ToolDialect` or
`WireDialect`); its parser arm in `cua-parse` (or its codec in `cua-vendors`) with a fixture
recorded by `dev/record-engine.sh`; its prompt in `cua-session/prompts/`; a row in the
dialect slug test of `cua-action/tests/shapes.rs`.

**Add a catalog entry**: a file `catalog/<id>.toml` with every field written (copy
`holo-3.1-4b.toml`), the `id` equal to the file stem; `tests/shipped.rs` in model-catalog parses
it. No code unless it needs a new engine kind, weight layout or dialect.

**Add an engine kind**: its `EngineKind` variant; the arm in `engine-supervisor::command`
(program, flags, socket flag, sandbox) with a row in the `command_is_pure` table; its
`EnginePaths` field and the settings key that fills it (`SpeechHost`: `ai.engine.speech_host.path`;
`KokoroFastApi`: `ai.engine.kokoro.python`).

**Add a speech model**: a `catalog/<id>.toml` as above, with `roles = ["speech_in"]` (or
`"speech_out"`) and the `speech` table in the matching direction (copy
`nemotron-3.5-asr-streaming.toml` or `kokoro-82m.toml`); no chat fields. The label is the
model's name and nothing else: the picker is a plain list, and `the_catalog_ranks_nothing` fails a
file that says best, recommended or the like. A model that runs on the CPU writes zero for all
three `vram` fields and the sandbox gets no GPU; a vLLM entry may not.

**Add a speech backend**: an impl of `SpeechToText` or `TextToSpeech` in its own crate or module,
a variant of `SttBackend` or `TtsBackend` in porter's inferd, a row in section 1 and in
`scripts/check-boundary.sh`; no GPL code is linked, so a TTS engine is a separate process with a
catalog engine profile.

**Add a resize rule**: its `ResizeRule` variant; its arms in `fit` and `image_tokens` with rows
in the reference table; nothing else (the map and the session read the rule).

**Add a seam implementation** (`EngineHost`, `ReadyProbe`, `GpuProbe`): a feature of
`engine-supervisor` (`systemd`, `process`, `nvidia`) that adds its dependency to the pinned block
first; the daemon (porter's inferd) chooses it.

## 7. Test harness

`ScriptedProvider` (feature `testing` of `model-provider`) plays a list of scripted turns and
records every `TurnRequest`; tests of prompt assembly assert on the recorded requests.
`ReplayProvider` plays a cassette. `FakeEngineHost`, `FakeReadyProbe` and `FakeGpu` (feature
`testing` of `engine-supervisor`) script the supervisor's seams. Tests never start a real
engine, never touch the GPU, never use a real systemd or bus, never read
`~/.cache/huggingface`, and never download a weight. Recorded fixtures come only from dev scripts
run by hand against the user's own engine.

## 8. Repo rules

- **Gate** (check every exit code):

  ```bash
  cargo fmt --all --check
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo test --workspace --all-features
  ./scripts/check-boundary.sh
  cargo deny check licenses
  ```

- **No `unsafe`** anywhere (`unsafe_code = "deny"`).
- **Excluded crates**: `speech-vad-silero` and `speech-host` are workspace `exclude`s and
  `deny.toml` `[graph] exclude`s, so the gate's clippy and test never build them (the runtimes
  they need are not in the pinned block). Build one by hand with
  `cargo build --manifest-path crates/<name>/Cargo.toml`; their `Cargo.lock` and `target/` stay
  untracked. They join the workspace if the runtime joins the pinned block and the gate can
  build it.
- **Dependencies** come from quire's pinned block (`docs/workspace-deps.toml` there) and the
  adopted lines of the spec (SPEC.md 1.4), copied verbatim, only the lines stoker names; a new
  one joins that file first. No `reqwest` (it cannot reach a Unix socket), no `async-openai`,
  `genai` or `rig`.
- **Licence** `MIT OR Apache-2.0`. Prompt files and fixtures taken from model cards carry their
  source and licence in the file header.
- **The wire is serde.** Every stored or wire type has a round-trip test; enums with data are
  adjacently tagged (`kind`/`v`); the catalog file is `ModelEntry`'s serde form and the
  cassette file is `Cassette::to_jsonl`.
- **Floats** appear nowhere: coordinates are `Coord(u32)`, scale is `Scale120`, temperature and
  every other sampling knob is `Milli` (inside a `Knob` when it may be left to the engine). The one
  exception is an embedding vector (`EmbedVector`, `f32`), which is floats end to end.
- **No untyped parameter bag.** Engine-specific knobs are fields of `EngineExtras` (one arm per
  flavor), never a JSON value merged into a request.
- **A model's words are untrusted.** Parsers drop what they do not know, and a point outside
  the frame is refused, never clamped.
- **No hard-coded proposed values.** The six `ai.engine.*` values and `ai.cua.history_frames`,
  `ai.cua.repair_attempts` are settings read by the daemon; `SupervisorConfig::default()` holds
  the proposed numbers only as the settings' defaults.
