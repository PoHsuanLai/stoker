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
| `model-provider` | `TurnRequest`, `Message`, `Part`, `ToolSpec`, `TurnEvent`, `TurnSink`, `Caps`, the `Provider` trait, `ProviderError`; feature `testing`: `ScriptedProvider` | none |
| `cua-parse` | `parse_text`, `parse_tool_calls`, `Parsed`, `Dropped`, `ParseLimits` | none |
| `cua-vendors` | the `WireCodec` trait and one codec per `WireDialect` (formerly `cua-wire`) | none |
| `cua-session` | `CuaSession`: history window, prompt assembly, parse with one repair, mapping to window space | none |
| `model-replay` | cassette format, `ReplayProvider`, `RecordingProvider` over a `CassetteSink`; `speech`: speech cassettes (audio as digests), `SpeechReplay`, `RecordingSpeech` | an injected sink |
| `model-catalog` | `ModelEntry` (chat caps or the `speech` table), `EngineProfile`, `parse_entry`, `merge_catalogs`; the shipped `catalog/*.toml` | none |
| `engine-supervisor` | the pure `step` and `budget`; `command` (catalog entry to `UnitSpec`); the seams `EngineHost`, `ReadyProbe`, `GpuProbe`; feature `testing`: fakes | none; the daemon fills the seams |
| `model-http` | `HttpEndpoint`, `HttpTarget` (Tcp, Unix, Tls), `AuthHeader`, the pure `SseDecoder`, `HttpClient` | yes (the client) |
| `model-openai-compat` | pure `encode_request` and `StreamDecoder`, plus `OpenAiCompat: Provider`; `audio`: `encode_speech_request`, `encode_transcription`, `PcmDecoder`, and `OpenAiSpeech` (both speech traits) | yes, through `model-http` |
| `speech-provider` | audio and text types, `SpeechToText`, `TextToSpeech`, `AudioSource`/`AudioSink`, `VoiceActivity`, `SpeechCaps`, the host wire and its framing; feature `testing`: `ScriptedStt`, `ScriptedTts`, `ScriptedVad` | none |
| `speech-vad` | `EnergyGate`, `Framer`, `level_of`, the `endpoint` machine | none |
| `speech-host-client` | `SpeechHostClient: SpeechToText` over the host's Unix socket | yes (the socket, when filled) |
| `speech-vad-silero` | `SileroVad: VoiceActivity` over `ort`; **excluded from the workspace** | the runtime |
| `speech-host` | the STT engine binary: sherpa-onnx (built without TTS) behind a Unix socket; **excluded from the workspace**, runs only as a confined engine unit | the runtime and the socket |

Allowed direct edges (checked by `scripts/check-boundary.sh`; dev-dependencies are outside it):

| Crate | May depend on |
| --- | --- |
| `cua-action`, `model-http` | nothing of ours |
| `vision-prep` | `cua-action` |
| `model-provider` | `cua-action`, `vision-prep` |
| `cua-parse` | `cua-action`, `model-provider` |
| `cua-vendors` | `cua-action`, `cua-parse`, `model-provider` |
| `cua-session` | `cua-action`, `cua-parse`, `cua-vendors`, `model-provider`, `vision-prep` |
| `model-replay` | `model-provider`, `speech-provider`, `vision-prep` |
| `model-catalog` | `model-provider`, `speech-provider` |
| `engine-supervisor` | `model-catalog` |
| `model-openai-compat` | `model-http`, `model-provider`, `speech-provider` |
| `speech-provider` | `model-provider` |
| `speech-vad` | `speech-provider` |
| `speech-host-client` | `model-provider`, `speech-provider` |
| `speech-vad-silero` (excluded) | `speech-provider`, `speech-vad` |
| `speech-host` (excluded) | `speech-provider` |

External boundaries: the pure crates (everything but the two io crates) never reach `tokio`,
`hyper`, `hyper-util`, `rustls`, `zbus`, `zvariant`, `reqwest`, `ureq`, `wayland-client`,
`wayland-backend`, `reis`, `atspi`, `oo7`, `ort`, `fastembed`, `rusqlite`, `notify`,
`cedar-policy` or `rmcp` (default features); `vision-prep` reaches `image` and `fast_image_resize`
only through `pixels`. The io crates never reach the second half of that list (a bus, a
compositor, a database, an inference runtime). No crate reaches a `porter-*` crate. No speech
crate reaches an audio device crate (`pipewire`, `libpulse-binding`, `libpulse-simple-binding`,
`cpal`: capture and playback belong to docket's `voiced`) or a TTS stack with a GPL grapheme
step (`espeak-rs`, `espeak-ng`, `espeak-ng-sys`, `piper-rs`): text to speech is a separate engine
process (Kokoro-FastAPI), and `speech-host` links sherpa-onnx built without TTS. The two
excluded crates are not workspace members; `check-boundary.sh` checks them from their own
manifests (their direct edges and the same lists), and they alone may reach `ort` and `sherpa-onnx`.

Downstream, porter's `inferd` takes `cua-action`, `vision-prep`, `model-provider`,
`cua-session`, `model-catalog`, `engine-supervisor`, `model-http`, `model-openai-compat`,
`speech-provider` and `speech-host-client` (SPEC.md 1.3, voice.md 2.2); docket's `voiced` takes
`speech-vad` and `speech-vad-silero`; porter-infer takes `cua-action` only. cuad, almanac, docket and sill do not name
stoker.

## 2. Modules

| Crate | Modules |
| --- | --- |
| `cua-action` | `space` < `geometry`, `target`, `text`, `keys` < `dialect`, `action` |
| `vision-prep` | `rule` < `frame_map` < `pixels` |
| `model-provider` | `units`, `ids` < `request` < `caps`, `event` < `provider` < `scripted` (feature `testing`) |
| `cua-parse` | `limits` < `outcome` < `parse` |
| `cua-vendors` | `step_result` < `codec` |
| `cua-session` | `model` < `session` |
| `model-replay` | `print` < `cassette` < `provider`, `speech` |
| `model-catalog` | `engine`, `entry` < `parse` |
| `engine-supervisor` | `state` < `unit`, `budget` < `step` < `host` < `fakes` (feature `testing`) |
| `model-http` | `target`, `auth` < `sse` < `client` |
| `model-openai-compat` | `codec` < `provider`; `audio` (`codec` < `provider`) |
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

// model-http: where a response body goes as it arrives.
pub trait BodySink: Send { fn chunk(&mut self, bytes: &[u8]) -> ChunkFlow; }
```

Closed sets stay enums: `CuaAction`, `Target`, `CuaDialect` (`WireDialect`, `TextDialect`,
`ToolDialect`), `ModelSpace`, `ResizeRule`, `Part`, `ToolSpec`, `TurnEvent`, `ProviderError`,
`EngineState`, `SupervisorIn`, `SupervisorOut`, `HttpTarget`, `AuthHeader`, `Flavor`,
`WireCodecs`, `WeightFiles`, `Licence`, `EngineKind`, `CatalogKind`, `PcmFormat`, `SttMode`,
`TranscriptEvent`, `LangChoice`, `LangSet`, `SpeechIo`, `HostIn`, `HostOut`, `Voiced`, `Endpoint`,
`SpeechFlavor`, `SpeechInteraction`.

## 5. What is frozen, what is built, what is stubbed

Frozen means: the public types, trait signatures, serde forms (adjacently tagged `kind`/`v`),
the catalog file format and the cassette file format below are the interface other repos build
on; a change is an edit of SPEC.md first. Every `todo!()` is listed in `FINDINGS.md`.

| Piece | State |
| --- | --- |
| `cua-action`: every type, `class`, `map_points`, the text and number limits | built, tested (round trips, pinned JSON, tables, compile-fail) |
| `vision-prep`: types and serde | built, tested; `fit`, `image_tokens`, `FrameMap` maths, `prepare` stubbed |
| `model-provider`: every type, `JsonText`, `ToolName`, `ImageBytes`, `ScriptedProvider` | built, tested |
| `cua-parse`: types and limits | built, tested; `parse_text`, `parse_tool_calls` stubbed |
| `cua-vendors`: trait, enum, `StepResult` | built; the codec bodies stubbed |
| `cua-session`: types, `begin` | built; `request`, `absorb` stubbed |
| `model-replay`: cassette format, round trip | built, tested; the providers and `RequestPrint::of` stubbed |
| `model-catalog`: types, `parse_entry`, `merge_catalogs`, `VramEstimate::need`, `gpu_need`, `holo-3.1-4b`, the five speech entries | built, tested |
| `engine-supervisor`: types, `Supervisor::new`, config defaults, fakes | built, tested; `step`, `budget`, `command` stubbed |
| `model-http`: types; the SSE decoder and client | types built; both bodies stubbed |
| `model-openai-compat`: types | built; codec and provider stubbed; `audio` types and signatures only, every body stubbed |
| `speech-provider`: every type, the checked names, `AudioChunk::{samples, duration}`, the host framing | built, tested (round trips, pinned JSON, redaction, duration table); the `testing` fakes' bodies stubbed |
| `speech-vad`: types, defaults | built, tested; `level_of`, `EnergyGate`, `Framer::push`, `endpoint` stubbed |
| `speech-host-client`: types | built; both trait bodies stubbed |
| `speech-vad-silero`, `speech-host` | skeletons (excluded from the workspace); every body stubbed |
| `model-replay::speech`: cassette format, round trip | built, tested; the prints, `SpeechReplay` and `RecordingSpeech` stubbed |

## 6. Recipes

**Add a backend** (a `Provider`): a crate `model-<name>` depending on `model-provider` and
`model-http`; a pure `codec` module (request JSON, stream to `TurnEvent`) tested against recorded
fixtures in `fixtures/<name>/` made by a dev script; the provider over `HttpClient`; its row in
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
- **Floats** appear nowhere: coordinates are `Coord(u32)`, scale is `Scale120`, temperature is
  `Milli`.
- **A model's words are untrusted.** Parsers drop what they do not know, and a point outside
  the frame is refused, never clamped.
- **No hard-coded proposed values.** The six `ai.engine.*` values and `ai.cua.history_frames`,
  `ai.cua.repair_attempts` are settings read by the daemon; `SupervisorConfig::default()` holds
  the proposed numbers only as the settings' defaults.
