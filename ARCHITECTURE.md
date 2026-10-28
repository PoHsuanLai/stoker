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
| `cua-action` | the provider-neutral action vocabulary: coordinate spaces (`CoordSpace`, `WindowSpace`, `ImageSpace`, `GridSpace`), `Point`/`Size`/`Rect`/`Length`, `Target`, `CuaAction<S>`, `ActionClass`, keys and chords, bounded text, dialect names (`CuaDialect`, `ModelSpace`), `ActionProfile` and the answer-shape options it carries (`CallFormat`, `AnswerShape`, `HistoryShape`, `ResolutionWording`) | none |
| `vision-prep` | `ResizeRule`, `fit`, `image_tokens`, `FrameMap`, `RawFrame`, `prepare` (feature `pixels`) | none; `pixels` is CPU only |
| `model-provider` | `TurnRequest`, `Message`, `Part`, `ToolSpec`, `TurnEvent`, `TurnSink`, `Caps`, the `Provider` trait (with `charges() -> Charges`), `ProviderError`; `money` (`Money`, `Charges`, `MoneyError`: only what a provider reports, no price table); the controls (`Sampling`, `ToolParallelism`, `EngineExtras`, `ThoughtSeal`, `Knob`); `embed` (`Embedder`, `EmbedTurn`, `EmbedRole`, `plan_batches`); `shape` (`Shape`, `Extract`, the schema, GBNF and regex conversions); `retry` (`RetryClass`, `next_wait`, `Retrying`); `logprobs` (`ChoiceScores`, `FirstTokenLogprobs`, `Logprob`, the `option_permille` helper); `sequence` (`check`); feature `testing`: `ScriptedProvider` | none |
| `cua-parse` | `parse_text`, `parse_tool_calls`, `parse_tool_calls_with` (`ToolContext`, `CursorAt`), `read_calls` (calls read from the reply text, `ParamKinds`), `answer_parts` (the `<think>` and `<action>` of a reply), `Parsed`, `Dropped`, `ParseLimits` | none |
| `cua-vendors` | the `WireCodec` trait and one codec per `WireDialect` (formerly `cua-wire`): tool declarations, decoders and result encoders for the Anthropic, OpenAI and Gemini computer-use tools | none |
| `cua-session` | `CuaSession`: history window, prompt assembly (`prompts/`), parse with one repair, mapping to window space; `TurnSettings`, `TranscriptSink`; `trait ActionDialect` with default methods (`wire`, `is_text`, `is_tool`, `system`, `schema`, `tools`, `retry`, `parse`), one impl per family and `spec(dialect) -> &'static dyn ActionDialect` (the only `match` on `CuaDialect` that picks behaviour; the dialect stays the serde key) | none |
| `model-replay` | cassette format, `ReplayProvider`, `RecordingProvider` over a `CassetteSink`; `wire`: wire cassettes recorded and replayed at the `Transport` seam (`RecordingTransport`, `ReplayTransport`, `ChunkPlan`); `check_sequence`; `speech`: speech cassettes (audio as digests), `SpeechReplay`, `RecordingSpeech` | an injected sink |
| `model-catalog` | `ModelEntry` (`family`, `cold_start_estimate_s`, `Capabilities`: `Modality` inputs and outputs with the detail tables `text_out`, `image_in`, `audio_in`, `audio_out`, `vector_out`, `actions_out`), `EngineProfile` (may narrow inputs and outputs; `context_arg: ContextArg` says how the engine takes its window), `TextOut` (`context`/`max_output` are what one engine line serves; `context_max`, `max_output_max`, `context_min` are the model's own limits, all optional), `Locality` (`OnDevice`, or `Remote { reach }` with `Reach { provider, model, wire }` (no prices)), `Slot`, `fits`, `slot_members`, `reachable`, `parse_entry`, `merge_catalogs`; the shipped `catalog/*.toml`. The older view (`roles`, `caps`, `sampling`, `speech`, `embed`) is derived and deprecated: use `Slot` | none |
| `engine-spec` | what differs between the engines, as `trait Engine` with default methods (`slug`, `disk_word`, `packages`, `package_paths`, `pointed`, `command`, `accepts`, `requires_gpu_estimate`), one unit struct per engine and `engine(kind) -> &'static dyn Engine` (the only `match` on `EngineKind` that picks behaviour; the kind stays the serde key); `ENGINES`, `engine_from_disk_word`; the launch values (`Launch`, `LaunchArgs`, `ProgramPath`, `EnginePaths` as a map by kind, `repo_dir`, `snapshot_dir`); `PackageSource` and its types; `check_entry` (what an entry needs of its engines) | none |
| `engine-supervisor` | the pure `step` and `budget`; `command` and `command_with_window` (catalog entry and a `Window`, the served one or a chosen one, to `UnitSpec`, or `MissingProgram`); the seams `EngineHost`, `ReadyProbe`, `GpuProbe`; feature `testing`: fakes | none; the daemon fills the seams |
| `model-http` | `HttpEndpoint` (with `Timeouts` and `ExtraHeader`s), `HttpTarget` (Tcp, Unix, Tls), `AuthHeader`, the pure `SseDecoder` and `NdjsonDecoder`, `ResponseHead`, `BodySink` (head, then chunks), `Exchange`, the `Transport` seam, `Upload` and `UploadTransport` (a POST of bytes), and `HttpClient: Transport + UploadTransport` (reqwest over TCP and Unix sockets, behind the `client` feature; TLS to a `Tls` target behind `tls`, with `TlsRoots` choosing the roots) | yes (the transport) |
| `engine-manifest` | the engines as data: `Manifest` (pinned `Row`s by `EngineKind`, `Arch` and `Backend`: `Artifact`s with hashes and sizes, `Delivery`, `PythonEnv` and its lock, the tree seal), `Host` (`Arch`, `Gpus`, `OsRelease` with `ships(&PackageSource)`, `BackendChoice`), `preference`, `Failure` (the codes of an install), `TreeSeal` and `SealLine` (the seal's line format; `tools/seal.py` pins the rows with it), `engine_from_slug` (the disk word of an engine, `Engine::disk_word`, read back to an `EngineKind`) | none |
| `engine-probe` | reading this computer for the manifest: `read_host`, `read_gpus` (the kernel's files and `os-release` under a given root), `Roots`, `InstallDir`, `Record` (where installs live and what one leaves), `Finder` (which engine is here, never running it), `Verifier` (the seal check before a person's own install starts), `seal_of`, `file_digest`, `entry_digests`. Every path and root is an argument | reads files |
| `engine-install` | getting an engine from the manifest: `Fetcher` (an `https` download over `model-http`'s `HttpClient` that resumes, follows redirects as `PinnedUrl`s, can be stopped, is counted and kept only if its size and hash are the pinned ones), `Installer` (space checked, unpacked in process by `tar` and `flate2`, sealed, renamed into place whole; or a Python environment filled by the pinned `uv` from a hashed lock), `install_package`, `EngineInstalls` (the jobs a program offers and follows), `install_for_everyone` and `HelperProgram` (the caller names `pkexec` and the helper by path), and the `helper` module with the `engine-helper` program (`install <engine> <backend>`, nothing else) | HTTPS, files, child processes |
| `model-wire` | the wire half of an endpoint: `ChatCodec`, `ChatDecoder`, `EmbedCodec`, `ErrorWire`, `CodecError`, and `Driver<C, T>`, the `Provider` (and `Embedder`) made of a codec and a transport | none (pure over the `Transport` trait) |
| `model-extract` | structured extraction as a pure machine: `choose` (native constraint, synthetic tool call or prompted), `ExtractSession`, `Extracted`, `ExtractFailure` | none |
| `genai-names` | the OpenTelemetry GenAI attribute, metric, operation and finish-reason names as constants; zero dependencies; no content attribute exists | none |
| `model-openai-compat` | the chat-completions codec `OpenAiCodec` (pure `encode_request`, `StreamDecoder`), the `Server` trait (what differs between servers, one default method each and one impl per server; `Flavor` names the server at run time), first-token log-probabilities (`logprobs`), `OpenAiCompat = Driver<OpenAiCodec, HttpClient>`; `audio`: `encode_speech_request`, `encode_transcription`, `PcmDecoder`, `OpenAiTranscription` (`SpeechToText`, vLLM) and `KokoroSpeech` (`TextToSpeech`) | none of its own: the HTTP is `model-http`'s `Transport` (the speech providers are generic over `T: UploadTransport`, an `HttpClient` by default) |
| `speech-provider` | audio and text types, `SpeechToText`, `TextToSpeech`, `AudioSource`/`AudioSink`, `VoiceActivity`, `SpeechCaps`, the host wire and its framing; feature `testing`: `ScriptedStt`, `ScriptedTts`, `ScriptedVad` | none |
| `speech-vad` | `EnergyGate`, `Framer`, `level_of`, the `endpoint` machine | none |
| `speech-host-client` | `SpeechHostClient: SpeechToText` over the host's Unix socket | yes (the socket, when filled) |
| `speech-vad-silero` | `SileroVad: VoiceActivity` over `ort`; **excluded from the workspace** | the runtime |
| `speech-host` | the STT engine binary: sherpa-onnx (built without TTS) behind a Unix socket; **excluded from the workspace**, runs only as a confined engine unit | the runtime and the socket |

`scripts/check-boundary.sh` holds a LAYERS table: every crate is **P** (pure: no `std::fs`/`env`/`process`/`net`,
no clock reads, no sleeps, no `tokio`, `hyper`, `reqwest`, `zbus` or `rustix`, no UI words), **C** (client/IO: may
do IO, no `Label`, `Help`, `Sheet` or `SettingRow`) or **A** (wiring). A crate depends on its own tier or below.
Today every crate is P except `engine-probe`, `engine-install`, `speech-host-client`, `speech-host` and
`speech-vad-silero` (C) and `speech-host-sherpa` (A). `model-http` is P with KNOWN rows: its feature-gated reqwest client (reqwest and tokio in
the source) is the one violation, removed when the client moves to its own crate (plan lane T1.2). `--self-test` seeds a `std::fs` call in a P crate and
checks the script fails; `--long-files` reports source files over 500 lines.

The direct edges in use (documentation; dev-dependencies are outside it):

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
| `engine-spec` | `model-catalog` |
| `engine-supervisor` | `engine-spec`, `model-catalog` |
| `engine-manifest` | `engine-spec`, `model-catalog` |
| `engine-probe` | `engine-manifest`, `engine-spec`, `model-catalog` |
| `engine-install` | `engine-manifest`, `engine-probe`, `engine-spec`, `model-catalog`, `model-http` (features `client`, `tls`) |
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
reach no HTTP stack and no runtime: `HttpClient`'s reqwest transport sits behind `model-http`'s
`client` feature, which only a daemon (or a loopback test) turns on, and their rows keep checking
default features. `model-http`
never reaches the second half of that list (a bus, a compositor, a database, an inference runtime);
`engine-install` reaches a runtime and an HTTP client through it and nothing of that second half, and `engine-manifest` and
`engine-probe` reach neither. No crate reaches a `porter-*` crate. No speech
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
| `cua-action` | `space` < `geometry`, `target`, `text`, `keys` < `dialect`, `profile`, `action` |
| `vision-prep` | `rule` < `frame_map` < `pixels` |
| `model-provider` | `units`, `ids` < `control`, `request` < `caps`, `event` < `provider` < `embed`, `shape` (`check`, `gbnf`, `pattern`, `schema`, `from_schema`), `sequence` < `retry` < `scripted` (feature `testing`) |
| `cua-parse` | `limits` < `outcome` < `common`, `scan`, `chord`, `verbs` < `ui_tars`, `profile` < `tools` < `parse` |
| `cua-vendors` | `step_result`, `args`, `safety`, `results` < `anthropic`, `openai`, `gemini` < `codec` |
| `cua-session` | `model`, `history`, `shown`, `prompt`, `dialect` (`ui_tars`, `tool`, `wire`), `window`, `transcript` < `session` |
| `model-replay` | `print` < `cassette` < `sequence` < `provider`, `speech`, `wire` |
| `model-catalog` | `modality`, `engine` < `capabilities` < `slot`, `view` < `entry` < `reach`, `file`, `check`, `legacy` < `parse` |
| `engine-spec` | `package`, `cache`, `paths` < `engine` < `vllm`, `llama`, `strata`, `speech_host`, `kokoro` < `check` |
| `engine-supervisor` | `state` < `unit`, `budget` < `step` < `host` < `fakes` (feature `testing`) |
| `engine-manifest` | `engine`, `host`, `release`, `failure`, `seal` < `manifest` < `rows` |
| `engine-probe` | `seal`, `host` < `layout` < `found`, `verify` |
| `engine-install` | `progress`, `space`, `pinned_url`, `extract` < `fetch` < `installer` < `pyenv`, `package` < `elevated`, `helper` < `jobs` |
| `model-http` | `target`, `auth`, `head` < `sse`, `ndjson` < `client` < `exchange` < `request`, `connect` < `transport` (feature `client`) |
| `model-wire` | `codec` < `driver` |
| `model-extract` | `mode` < `session` < `shaped` |
| `genai-names` | `lib` |
| `model-openai-compat` | `server`, `logprobs`, `codec` < `provider`; `audio` (`codec` < `provider`) |
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
| how one model's calls differ from its dialect's convention | `cua-action::profile` (`ActionProfile`, in the catalogue entry's `actions_out`); read by `cua-parse::profile`, shown by `cua-session::shown` |
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
| turning a catalog entry into a unit to run, with the context window as an input | `engine-supervisor::command_with_window` (`Window::{Serve, Chosen}`) |
| engine lifecycle and the VRAM budget | `engine-supervisor::step`, `budget` (the only place) |
| the SSE framing | `model-http::SseDecoder` |
| the NDJSON framing | `model-http::NdjsonDecoder` |
| a response's status and the few headers a codec reads (`Location` and `Content-Range` for a download) | `model-http::ResponseHead` (delivered by `BodySink::head` before any chunk) |
| the pinned engine builds and their hashes | `engine-manifest::Manifest` |
| the distro packages of an engine, the paths a package puts it at | `engine-spec::Engine::packages`, `package_paths` |
| an engine's word on disk, in a record and in the helper's arguments | `engine-spec::Engine::disk_word` (`engine-manifest::engine_from_slug` reads it back) |
| what an engine's command line, weights and GPU estimate are | `engine-spec::Engine` (`command`, `accepts`, `requires_gpu_estimate`); `check_entry` for a parsed entry |
| the tree seal's text | `engine-manifest::SealLine`, `TreeSeal` (made on disk by `engine-probe::seal_of`; pinned by `tools/seal.py`) |
| where engine installs live, the record, which engine is here | `engine-probe::Roots`, `Record`, `Finder` |
| downloading, unpacking and sealing an engine install | `engine-install::Installer`, `Fetcher` (the only place) |
| the address a download may follow | `engine-install::PinnedUrl` (`https` with a host) |
| delivery of one exchange (TCP, Unix, TLS, in process, record, replay) | `model-http::Transport` |
| a wire's request, reply and error format | `model-wire::{ChatCodec, EmbedCodec}`; the OpenAI-compatible one is `model-openai-compat::OpenAiCodec` |
| what differs between OpenAI-compatible servers | `model-openai-compat::Server` (picked by `Flavor`) |
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
catalog's `roles` are `CatalogKind`s (stoker's copy of porter's `AiKind`, same slugs); since C1
(agent-spec/capabilities.md) they are derived from the declared capabilities, and `Slot` is the
name a person picks by.

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

// speech-provider: one per backend (SpeechHostClient, OpenAiTranscription, KokoroSpeech, SpeechReplay, RecordingSpeech);
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
`WeightFiles`, `Licence`, `EngineKind`, `CatalogKind`, `PcmFormat`, `SttMode`,
`TranscriptEvent`, `LangChoice`, `LangSet`, `SpeechIo`, `HostIn`, `HostOut`, `Voiced`, `Endpoint`,
`SpeechInteraction`, `Framing`, `Verb`, `BodyKind`, `RouteRoot`, `Knob`, `EngineExtras`, `ThoughtSeal`, `ToolParallelism`, `Shape`, `ExtractMode`, `RetryClass`, `WireBody`, `WireEnd`, `ChunkPlan`, `ShapeWithTools`.

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
| `model-catalog`: types, `parse_entry`, `merge_catalogs`, `VramEstimate::need`, `gpu_need`, `SamplingDefaults`, `holo-3.1-4b`, the five speech entries, `Modality`, `Capabilities::on_engine`, `Slot`, `fits`, `slot_members`, `reachable`, the eight hosted entries (Claude, Gemini, Kimi, GPT), older-shape files | built, tested |
| `engine-supervisor`: types, `Supervisor::new`, config defaults, fakes, `step`, `budget`, `command` | built, tested (the lifecycle table, the budget table with the in-turn window, `command` for vLLM, llama-server, the speech host and Kokoro) |
| `model-http`: types, `ResponseHead`, `BodySink::head`, `Exchange`, `Transport`, `Timeouts`, `ExtraHeader`, `HttpError`, the SSE and NDJSON decoders, `Transport for HttpClient` (feature `client`) | built; round-trip, pinned-JSON, proptest and loopback-socket tested (TLS is feature `tls`, tested against a scratch CA on loopback; the egress proxy answers `Connect`) |
| `engine-spec`: `Engine`, the five impls, `engine`, `check_entry`, `EnginePaths` | built, tested (the words, the defaults on a test engine, the entry checks moved from `parse_entry`) |
| `engine-manifest`, `engine-probe`, `engine-install`: moved from porter's `porter-engines` and `porter-engines-install` (porter `f0539103`) | built, tested (the rows, the seal against `tools/seal.py`'s vector, the Finder and Verifier on scratch directories, the fetcher, installer, jobs and helper against a loopback `https` host with a scratch CA); porter, detent and the installer scripts still use their own copies until they move over |
| `model-wire`: `ChatCodec`, `ChatDecoder`, `EmbedCodec`, `ErrorWire`, `CodecError`, `Driver` | built, tested with a scripted transport and a line codec (head decides, framer, decoder fault, Retry-After, stop, every transport failure, embed checks) |
| `model-extract`: `ExtractMode`, `ToolsPresent`, `ExtractSession`, `Extracted`, `ExtractFailure`, `choose`, `request`, `absorb` | built, tested |
| `genai-names`: every name, `Operation`, `Finish` | built, tested (names are values, so there is no stub) |
| `model-provider` amendment: `Sampling`, `EngineExtras`, `Knob`, `ThoughtSeal`, the `embed`, `shape`, `retry` and `sequence` types, `OutputShape::{Gbnf, Choice}`, `Constraint::{Gbnf, Choice}`, `ProviderError::Server`, `TurnUsage.cached`, `ModelInfo.{loaded_context, trained_context}` | built, round-trip and pinned-JSON tested; `plan_batches`, `EmbedEnd::check`, the `Shape` conversions and checker, `retry_class`, `next_wait`, `Retrying`, `sequence::check` stubbed |
| `model-openai-compat`: types, the `Server` trait, `OpenAiCodec` (`encode_request`, `encode`, `classify`, `describe`, `parse_models`, `encode_embed`, `decode_embed`), `StreamDecoder` | built, tested (golden bodies, the flavor table, error classes, wire cassettes under every chunking, one loopback-socket run of the whole stack); the `audio` codec (`encode_speech_request`, `encode_transcription`, `decode_transcription`, `PcmDecoder`) is built and tested; `OpenAiTranscription` and `KokoroSpeech` are built |
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
`scripts/check-boundary.sh` (RULES and LAYERS) and in section 1; its variant in porter's
`inferd::AdapterModel`.

**Add a dialect**: the variant in `cua-action::dialect` (`TextDialect`, `ToolDialect` or
`WireDialect`); a unit struct and `impl ActionDialect` in `cua-session/src/dialect/` that overrides
only what differs (`parse` is required; `system`, `schema`, `tools`, `retry`, `is_text`, `is_tool`
and `wire` default), and its arm in `cua-session::spec`; for a tool dialect, its verb table in
`cua-parse/src/verbs.rs` (synonyms, extra verbs, whether the function name is the verb) and its arm
in `Verbs::of`; or its codec in `cua-vendors` for a wire; a fixture recorded by
`dev/record-engine.sh`; its prompt in `cua-session/prompts/`; a row in the dialect slug test of
`cua-action/tests/shapes.rs`. A test dialect needs only its impl (`cua-session/tests/it/echo.rs`).

**Add a model that speaks a variant of a tool dialect**: no code. There is no standard for a
screen model's actions; the convention is Anthropic's computer-use tool as Qwen copied it
(`ToolDialect::QwenComputerUse`: an `action` verb and a `coordinate`), and most open screen
models are Qwen fine-tunes that rename a verb, drag from the cursor or take a scroll axis as
an argument. State the differences in the entry's `actions_out` as `profile = { aliases = [..],
drag = .., scroll = .. }` (see `ui-mate-27b.toml`); the parser reads through it and the session
shows the model its own verbs. A difference the profile cannot say (a verb that holds a key
down) is a new `VerbAlias` or `CuaAction` variant, which is code.

**Add a model trained on its own answer shape**: no code. A fine-tuned model does best when it is
shown the shape it was trained on, and its reference harness is the source (read what it puts in the
system message, how it reads the reply and what it replays as history). Each difference is one
typed option in the entry, and an entry that names none behaves as before:
`text_out.tools = "client_parsed"` with the profile's `calls = "qwen3_coder_xml"` (or `hermes_json`)
when the reference reads the calls out of the raw reply text (the request then declares no tools, the
system message carries the tool block generated from the same schema, `cua-parse::read_calls` reads
the `<tool_call>` blocks from the reply and from the reasoning, and the engine line must carry no
`--tool-call-parser`, `--reasoning-parser` or `--enable-auto-tool-choice`: a server parser strips the
text the model reasons and calls in; `parse_entry` refuses the one without the other);
`answer = "think_action_call"` for a reply of `<think>`, one `<action>` sentence and the calls (stated
in the system message, restated in a repair, the action sentence is the step's words);
`resolution = "grid_cells"` when the reference tells a 0-999 grid it is 1000x1000;
`history = "own_replies"` when the reference replays the model's own replies as its turns, with the
frames `image_in.per_prompt` allows (older frames become a collapsed placeholder). Pin each choice
in the entry's header comment with the reference line it copies, and add the entry's row to the
`shipped.rs` test for it. A wording the reference uses that none of these says is a new variant of
the enum in `cua-action::trained`, never a prompt file for one model. When a model fails under the
harness, turn the options on one at a time to learn which one matters.

**Add a catalog entry**: a file `catalog/<id>.toml` with every field written (copy
`holo-3.1-4b.toml`), the `id` equal to the file stem. It declares `inputs` and `outputs`
(`text`, `image`, `audio` in; `text`, `image`, `audio`, `vector`, `actions` out) and exactly the
detail tables those need: `text_out` for text out, `image_in`, `audio_in`, `audio_out`,
`vector_out`, `actions_out`; the parser refuses a table without its modality and a modality
without its table. An engine block may narrow with `inputs`/`outputs` (a subset of the model's)
when that engine has no path for a modality. Slots follow from the capabilities, nothing is
declared for them. Check each claim against the model card; add the id to `IDS` in
`tests/shipped.rs`. No code unless it needs a new engine kind, weight layout or dialect.
Files in the older shape (`roles`, `speech`, `embed`) still load.

A text entry states two kinds of length. `context` and `max_output` are what its engine line
serves today. The model's own limits are separate and optional: `context_max` (from the card or
the pinned `config.json`'s `max_position_embeddings`, or `text_config.` for a multimodal one),
`max_output_max` (only where the card states one) and `context_min` (below it the model is not
useful); the header comment names each number's source. The parser refuses a served window or
reply above its maximum and a minimum above the maximum; a served window under `context_min` is
allowed, it is an old line. A vLLM line does not write `--max-model-len`: it names
`context_arg = { kind = "flag", v = "--max-model-len" }` and `engine-supervisor` appends the
flag and the window (`Window::Serve` is the entry's `context`, `Window::Chosen` the caller's, as
inferd picks from the limits and the free GPU memory). An engine that sets the window per request
says `context_arg = { kind = "per_request" }` and gets nothing at start.

**Add an engine kind**: its `EngineKind` variant (the one closed set of engines: llama.cpp, vLLM, Strata and the speech engines; nothing else in stoker keeps its own list); a unit struct and `impl Engine` in `engine-spec` that overrides only what differs (`slug` is required; the disk word, packages, `command`, `accepts`, `requires_gpu_estimate` and `pointed` default), its arm in `engine-spec::engine` and its entry in `ENGINES`; the settings key that fills its `EnginePaths` program (`SpeechHost`: `ai.engine.speech_host.path`; `KokoroFastApi`: `ai.engine.kokoro.python`). A test engine needs only its impl (`engine-spec/tests/it/echo.rs`).

**Add a hosted entry**: a `catalog/<id>.toml` with `locality = { kind = "remote", v = { reach = [...] } }`
(each reach: `provider` as porter's provider file id, `model` the provider's own id, `wire`; no
price: the catalogue keeps none), `source = { kind = "hosted" }`, zero `vram`, no `[[engine]]`,
`cold_start_estimate_s = 0`, and the usual capability tables (tool calling is required for the Text
slot). Every id and limit is read from the company's own docs and the gateway's model data,
with the URLs and the date in a comment in the file; a fact that cannot be read leaves the entry
out. `reachable(entry, granted, wires)` picks the reach: a direct provider before the gateway
(`openrouter`), only among granted providers and wires this build speaks.

**Add a speech model**: a `catalog/<id>.toml` as above: `inputs = ["audio"]`, `outputs = ["text"]`
with `audio_in` (and a `text_out` with zero `context` and `max_output`, since it has no chat
window), or `inputs = ["text"]`, `outputs = ["audio"]` with `audio_out` (copy
`nemotron-3.5-asr-streaming.toml` or `kokoro-82m.toml`). The label is the
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
  adjacently tagged (`kind`/`v`); the catalog file is `EntryFile` (`ModelEntry` serialises to it) and the
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

## 9. Install

`scripts/install.sh` installs `catalog/*.toml` under `PREFIX/share/stoker/catalog` (inferd reads
`/usr/share/stoker/catalog` and `$XDG_DATA_HOME/stoker/catalog`, so use `--prefix /usr` for a
machine-wide install). Nothing is built: stoker is libraries and data. Flags: `--prefix DIR`
(default `/usr/local`), `--destdir DIR` (for packages), `--user` (under `~/.local/share`, no root),
`--dry-run` (print every action); `--no-build`, `--bin-dir`, `--sandbox` and `--distro` are
accepted for the same command line as the other repos and do nothing here. Root steps use `$SUDO`,
else sudo, doas or pkexec, else they are printed. The speech host (`speech-host-sherpa`) is not
installed: it needs the sherpa libraries that `dev/build-sherpa.sh` fetches. If it ever builds, the build is linked by the system's own linker; clang and mold are for working on the code, and an install never needs them.
