# Findings

Open items and standing facts. An entry names the condition that closes it.

## S0: Holo-3.1-4B (read-only, from the Hugging Face hub files; no weights downloaded)

Read: `chat_template.jinja`, `preprocessor_config.json`, `config.json`, the model API record
(commit `8c88265a5a159bfd1492db9243733dd2e6e04a6e`), and the README (which states the licence and
nothing else we need).

| Fact | Source | Settles |
| --- | --- | --- |
| Architecture `Qwen3_5ForConditionalGeneration`, base Qwen3.5-4B (the card also lists Qwen3.5-0.8B, 9B and Qwen3.6-35B-A3B as bases); 32 layers, every fourth one full attention (8 of 32), 4 KV heads of head size 256 | `config.json` | the KV estimate: 8 layers x 4 heads x 256 x 2 (K and V) x 2 bytes = 32 KiB per token = **32 MiB per 1k tokens**; the 24 linear-attention layers keep a fixed-size state inside the overhead |
| Vision tower: patch size 16, spatial merge size 2, temporal patch size 2 | `config.json`, `preprocessor_config.json` | `PatchFactor(32)` (16 x 2), not the 28 of Qwen2.5-VL and UI-TARS-1.5 |
| Processor `Qwen3VLProcessor`; `size.shortest_edge = 65536`, `size.longest_edge = 16777216` (Qwen's processors name min and max pixels this way) | `preprocessor_config.json` | `min_pixels = 65536`, `max_pixels = 16777216` in `catalog/holo-3.1-4b.toml` (the earlier proposal had 3136 and 1003520) |
| Image normalisation mean and std 0.5 | `preprocessor_config.json` | engine-side; stoker only resizes and encodes |
| Chat template: tools are rendered as JSON function definitions; calls are written as Qwen3-coder XML (`<tool_call><function=name><parameter=key>value</parameter></function></tool_call>`); `<think>` blocks; image and video via `<|vision_start|>`; **no computer-use function and no coordinate convention** anywhere in it | `chat_template.jinja` | tools are `server_parsed` (vLLM `--tool-call-parser qwen3_coder`, as H Company's serving page uses); the action schema is whatever the session's prompt declares |
| The README gives no action schema, coordinate convention or image size | README | nothing; see below |
| Licence Apache-2.0 | README, API record | `licence = open(Apache-2.0)` |

Decisions taken from it:

- **`ToolDialect` gains `Holo31`** beside `QwenComputerUse`. The template does not name Qwen's
  `computer_use` function, so nothing shows Holo uses that argument schema. `QwenComputerUse`
  stays for models that do (Qwen3-VL); the catalog entry for Holo says `holo31`. Both are
  frozen; `cua-parse` fills `Holo31` from calls recorded by `dev/record-engine.sh`, and the
  fixtures decide whether the two parsers share code.
- **`GridMax(1000)` for Holo is the Qwen3-VL family's convention and is unverified**: neither
  the template nor the README states it. `images.space` in the catalog entry is the single place
  to change it; the first recorded step (`dev/cua-step-holo.sh`, by hand) confirms or corrects it.
- **`PatchFactor(32)`, `min_pixels = 65536`, `max_pixels = 16777216` are read from the files.**
- `kv_per_1k_ctx_mib = 32` is derived from `config.json` (above); `weights_mib = 10400`
  and `overhead_mib = 1500` are the proposal's estimates; a measurement in engine fill replaces them.
- The `llama_server` engine block is left out of the entry: no GGUF with a multimodal projector
  is known for the 4B. It is added when one exists.

## S1: speech catalog entries (read-only, from the Hugging Face API records and model cards; nothing downloaded)

| Fact | Source | Settles |
| --- | --- | --- |
| The sherpa-onnx export of Nemotron 3.5 streaming is `csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11` (commit `ab43d895f5985b1bbab8b6eac8607fcdc05343f3`); the proposal's `csukuangfj/...` does not exist. Files: `encoder.int8.onnx`, `decoder.int8.onnx`, `joiner.int8.onnx`, `tokens.txt`, test wavs | model API record | `source` of `catalog/nemotron-3.5-asr-streaming.toml`, `WeightFiles::SherpaDir` |
| Licence OpenMDW-1.1 (`license: other`, `license_name: openmdw-1.1`); 40 language-locales, written with the tags `en-US`, `zh-CN`, `nb-NO`, ... (there is no `zh-TW`); punctuated output; language detection with `target_lang=auto` | NVIDIA model card | `licence`, the 40 `langs` in the entry |
| Kokoro-82M: `hexgrad/Kokoro-82M`, commit `f3ff3571791e39611d31c381e3a41a3af07b4987`, Apache-2.0 | model API record | `catalog/kokoro-82m.toml` |
| Whisper large-v3 `06f233fe06e710322aca913c1bc4249a0d71fce1` (Apache-2.0; the commit the local cache holds), large-v3-turbo `41f01f3fe87f28c78e2fbf8b568835947dd65ed9` (MIT), Breeze-ASR-25 `cffe7ccb404d025296a00758d0a33468bec3a9d0` (Apache-2.0) | model API records | the three vLLM entries |

Decisions taken from it:

- **`CatalogKind::Speech` is split into `SpeechIn` and `SpeechOut`** (slugs `speech_in`, `speech_out`):
  one speech table has one direction, and the pickers choose a model per direction. porter's `AiKind`
  makes the same split (voice.md C-V2), and inferd's bridge owns the total mapping test.
- **`ModelEntry.caps` is `Option<Caps>` and `speech` is `Option<SpeechCaps>`.** A speech entry
  writes no chat fields, a chat entry no `speech` table; `parse_entry` checks both against the roles
  (`SpeechRoleWithoutSpeechTable`, `SpeechTableWithoutSpeechRole`, `SpeechDirectionMismatch`,
  `BothSpeechDirections`, `ChatFieldsWithoutChatRole`). A missing chat field is still a `Toml`
  error that names it. Callers read `entry.caps` as an `Option`.
- **`SpeechCaps` is one struct with the direction-specific fields in `SpeechIo`**, flattened into
  the `speech` table with `dir = "in"` or `dir = "out"` as the spec's file shows (`input` for in,
  `output` and `voices` for out), so an in-model cannot carry voices. This is the one internally
  tagged enum in the repo, because the file format fixes the `dir` key.
- **`VramEstimate::gpu_need` is `Absent` exactly when all three numbers are zero**; `command` gives
  such an entry a sandbox with `GpuAccess::Absent`, and `parse_entry` refuses a vLLM engine with
  an all-zero estimate (`VllmWithoutVram`).
- **The catalog ranks nothing** (the user's answer to V4: a plain list, no Best or recommended
  tier anywhere). Labels are the model's name; `the_catalog_ranks_nothing` fails a file that
  says best, recommended, accurate, fastest or premium. The UI sorts however it likes; the data
  holds no order.
- **Whisper and Breeze vram numbers are estimates** (fp16 weights from the parameter counts,
  1500 MiB overhead, a 0.20 or 0.30 `--gpu-memory-utilization`); spike V-L replaces them.
  `max_audio_ms` for them is the 30 s window; Nemotron's 120 s is the proposal.
- **`SpeechHost` args carry `{socket}`** and the weights directory is added by `command` (the
  `--socket` and `--model-dir` flags of `speech-host`). Kokoro's `--uds {socket}` is unverified until
  the Kokoro dev script (V-T) runs by hand.
- **Frozen deviation from the spec text**: the host wire also carries framing helpers
  (`encode_frame`, `frame_length`, `decode_frame`), built, not stubbed: they are pure and both
  ends need the same 1 MiB cap. `ScriptedTts` makes silence only (no floats, so no sine).

## Stubs behind frozen interfaces

Every `todo!()` in the repo (the rig amendment added 27, see `The rig amendment` below; the fill wave F1 lanes remove theirs, `stoker-shape` removed 19 (see `Fill F1: stoker-shape` below) and the F2 lane `stoker-driver` removed 13 more, see `Fill F2: stoker-driver` below; 29 remain). Each is a signature other repos build on; the body arrives with
the work in the "Closes when" line of its crate.

### `cua-parse` (0, filled in wave F1)

Both entry points are built. `parse_text` reads the UI-TARS-1.5 grammar (the model-card examples are in `fixtures/ui_tars_15/`); `parse_tool_calls` reads `QwenComputerUse` (Qwen's `computer_use` schema) and `Holo31`.

Still open: the `Holo31` schema is ours, provisional. The chat template names no computer-use function, so the parser reads Qwen's schema plus flat verbs (`click`, `drag`, `move`, `finish`, `ask`, `observe`; a point as `x` and `y` or `coordinate`), and `fixtures/holo_31/` and `fixtures/qwen_cu/` are hand-written from those schemas, not recorded. Closes when `dev/record-engine.sh` records real calls from Holo-3.1-4B and the fixtures and the verb tables are corrected to match. The nightly targets are `crates/cua-parse/fuzz/` (`dev/fuzz-cua-parse.sh`, needs `cargo-fuzz`); `never_panics` (proptest) runs on stable.

### `cua-session` (2)

- `src/session.rs`: CuaSession::request: system prompt, history window, observation, frame
- `src/session.rs`: CuaSession::absorb: parse, one repair, map, history

Closes when `request` and `absorb` pass the ScriptedProvider tests (history keeps the last N frames, one repair then unparseable, out-of-frame becomes dropped); prompt files in `prompts/` carry their model-card attribution.

### `cua-vendors` (3)

- `src/codec.rs`: WireCodecs::tools: one declaration per vendor
- `src/codec.rs`: WireCodecs::decode: one decoder per vendor
- `src/codec.rs`: WireCodecs::results: stop-at-first-failure text per vendor

Closes when the first cloud backend is turned on (the user chose open models only).

### `engine-supervisor` (0, filled in waves F1 and F2)

`step` and `budget` (F1), `command` (F2): see `Fill F2: stoker-driver`. Still open: the io features (`systemd`, `process`, `nvidia`), see `Open` below.

### `model-http` (0, filled in waves F1 and F2)

`SseDecoder`, `NdjsonDecoder` (F1) and `Transport for HttpClient` over hyper behind the `hyper` feature (F2), with the loopback Unix-socket and TCP tests the stub named. TLS and the egress proxy are not built (see `Fill F2: stoker-driver`).

### `model-openai-compat` (8: the `audio` bodies; the chat codec was filled in F1 and F2)

- `src/audio/codec.rs`: encode_speech_request: model, input, voice, response_format pcm, stream true
- `src/audio/codec.rs`: encode_transcription: WAV header over the samples, form fields, boundary
- `src/audio/codec.rs`: decode_transcription: the `text` field of the JSON
- `src/audio/codec.rs`: PcmDecoder::feed: join the carry, keep the partial sample, number the chunk
- `src/audio/provider.rs`: OpenAiSpeech::describe (in): GET /models, caps from the catalog entry
- `src/audio/provider.rs`: OpenAiSpeech::transcribe: drain the audio, POST multipart, one Final event
- `src/audio/provider.rs`: OpenAiSpeech::describe (out): GET /models and /audio/voices
- `src/audio/provider.rs`: OpenAiSpeech::speak: POST /audio/speech, PcmDecoder into the sink

The chat codec passes its golden requests, the conformance list of `research-rig.md` 3.2, 3.3 and 3.7 (hand-written frames and wire cassettes under every chunking), and still waits for recorded fixtures from vLLM and llama-server (`dev/record-engine.sh`, run by hand, recorded at the `Transport`); `Flavor::quirks` is a pinned table whose rows marked "to verify" in `quirks.rs` are settled by the first recorded fixture of that engine; the `audio` bodies close with `model-http` gaining a multipart or raw-bytes POST (`post_json` cannot carry a WAV file), then `encode_speech_request` and `encode_transcription` pass golden bodies, `PcmDecoder` its chunking proptest, and `OpenAiSpeech` a loopback test against fixtures recorded by `dev/record-speech.sh` (by hand, against the user's own vLLM and Kokoro).

### `model-replay` (0, filled in wave F1)

Prints, sequences, the providers, the speech replay and the wire transports are built (`blake3` is in the workspace). In F2 the header gained the model's context sizes and optional speech caps (ask 18).

### `model-wire` (0, filled in wave F2)

`Driver::{describe, turn, embed}`: see `Fill F2: stoker-driver`.

### `model-extract` (0, filled in waves F1 and F2)

`choose`, `request` and `ExtractSession::absorb` (the F1 `absorb_for`, renamed by ask 49 in F2); `named_tool_refused_on_llama_server` passes in `model-openai-compat`'s `encode_request` tests.

### `speech-host` (2, excluded crate)

- `src/lib.rs`: parse_args: the four flags, each once, nothing else
- `src/lib.rs`: serve: sherpa-onnx online recognizer, one utterance at a time over host_wire

Closes when spike V-H shows an offline source build of sherpa-onnx with `-DSHERPA_ONNX_ENABLE_TTS=OFF` (no build-time download) and the Nemotron model id, `sherpa-onnx` joins the pinned block, and a loopback test drives `Hello`, `Begin`, `Audio`, `End` and `Cancel` with a scripted recognizer.

### `speech-host-client` (2)

- `src/lib.rs`: SpeechHostClient::describe: Hello, read the models
- `src/lib.rs`: SpeechHostClient::transcribe: Begin, pump audio and events, End or Cancel on drop

Closes when `tokio` (pinned block: `net`, `io-util`) joins the workspace lines, then both bodies pass a loopback Unix-socket test against a fake host that speaks `host_wire`.

### `speech-provider` (4, feature `testing`)

- `src/testing.rs`: ScriptedStt::transcribe: pull audio, push each event once its index passes
- `src/testing.rs`: ScriptedTts::speak: silent chunks until the audio length, stop on Flow::Stop
- `src/testing.rs`: ScriptedVad::push: the next verdict, with probability 1000 or 0
- `src/testing.rs`: ScriptedVad::reset: back to the start of the script

Closes when the fakes pass tests of their own: events arrive once the pulled audio passes their index, `Flow::Stop` ends a synthesis, a request is recorded before its script plays.

### `speech-vad` (5)

- `src/level.rs`: level_of: integer RMS, dBFS from a table, clamp -60..0 to 0..=1000
- `src/energy.rs`: EnergyGate::push: level_of against the threshold, hangover frames
- `src/energy.rs`: EnergyGate::reset: no loud frame seen
- `src/framer.rs`: Framer::push: decode S16, join the carry, cut 512-sample frames
- `src/endpoint.rs`: endpoint: Waiting to InSpeech on speech, Trailing on silence, Ended after silence_end

Closes when `level_of` passes a table (silence is 0, full scale is 1000, -40 dBFS is 333) with no floats, `EnergyGate` its hangover table, `Framer` a proptest (any chunking gives the same frames, the remainder carries), and `endpoint` the state table of voice.md 4.2 including `min_speech` and `NoSpeech`.

### `speech-vad-silero` (3, excluded crate)

- `src/lib.rs`: SileroVad::load: ort session over the ONNX file, zeroed [2,1,128] state
- `src/lib.rs`: SileroVad::push: run the model on the frame, carry the state, threshold the probability
- `src/lib.rs`: SileroVad::reset: zero the recurrent state

Closes when `ort` (2.0.0-rc.13, the line fastembed resolves) joins the pinned block and the Silero v6.2.1 file, supplied by the daemon's path, gives the reference probabilities on a synthetic fixture under `fixtures/audio/`.

### `vision-prep` (0, filled in wave F1)

`fit` matches the reference `smart_resize` on every row of `fixtures/smart_resize.csv` (made by `dev/smart-resize-vectors.py` from qwen-vl-utils); `image_tokens`, `FrameMap` and `prepare` are built and tested, with the round-trip proptest. `BelowMinimum` is returned only when a rule's budget floors an image to less than one patch.

Also not yet present, and not `todo!()`:

- `fixtures/audio/` (synthetic files) and `dev/record-speech.sh`: close with the fills above.
- `command` rows for `SpeechHost` (`paths.speech_host`, `--socket`, `--model-dir`) and
  `KokoroFastApi` (`paths.kokoro_python`, uvicorn's `--uds`) join the `command_is_pure` table
  with `command`'s fill; zero-vram entries get `GpuAccess::Absent`.
- The settings `ai.engine.speech_host.path` and `ai.engine.kokoro.python` (the quire agent owns the
  design/22 rows).

- The io feature flags of `engine-supervisor` (`systemd`, `process`, `nvidia`) and the
  `ChildProcesses` and `SystemdUnits` hosts: they join with their dependencies (`zbus` for the
  transient units, `tokio` with `process`) once the pinned block carries the tokio features of
  SPEC.md 1.4. Closes with engine-supervisor's io fill.
- `cua-session/prompts/{ui_tars_15,qwen_computer_use,holo_31}.txt`: taken from Apache-2.0 model
  cards with attribution in the file header. Closes with the `CuaSession::request` body.
- The `fuzz/` crate (non-workspace, nightly `cargo-fuzz`) and `dev/` scripts
  (`smart-resize-vectors.py`, `record-engine.sh`, `cua-step-holo.sh`). Close with their crates'
  fills.
- Wire fixtures under `fixtures/`. Recorded by a dev script against the user's own engine, never
  generated in CI.

## The rig amendment (2026-10-03)

Interface changes made before any fill wave, from `research-rig.md` section 7 items 1 to 6.
Types, traits, signatures, docs, pure tables and tests only; the new bodies are the 27 stubs above.

- **Wire and Transport split.** `model-http` gains `ResponseHead`, `BodyKind`, `BodySink::head`,
  `Exchange`, `Framing`, `Verb`, `RouteRoot`, `Timeouts`, `ExtraHeader`, `HttpError::Rejected`,
  `NdjsonDecoder` and the `Transport` trait (it replaces `HttpClient::{get, post_json}`);
  `HttpEndpoint` gains `headers` and `timeouts` (no defaults: the daemon reads them from
  settings). New pure crate `model-wire` (`ChatCodec`, `ChatDecoder`, `EmbedCodec`, `ErrorWire`,
  `Driver`). `model-openai-compat` is re-based on it: `OpenAiCodec`, `Flavor::quirks()`,
  `OpenAiCompat = Driver<OpenAiCodec, HttpClient>`. The spec's `pub type OpenAiCompat` cannot keep
  its inherent `new` (a foreign generic type), so the constructor is `OpenAiCodec::provider(client)`.
  `StreamDecoder::feed` takes the frame text (`&str`), not an `SseEvent`.
- **`model-provider`.** `Sampling` (temperature, top_p, top_k, min_p, repeat_penalty, seed, each a
  `Milli` or a `Count`, optional ones as `Knob`), `ToolParallelism`, `EngineExtras` (one arm per
  flavor: `LlamaServer`, `Vllm`, `Ollama`), `ThoughtSeal` and `Part::Thought { text, seal }`,
  `TurnEvent::ThoughtSealed`, `OutputShape::{Gbnf, Choice}`, `Constraint::{Gbnf, Choice}`,
  `ProviderError::Server(ServerStatus)`, `TurnUsage.cached`, `ModelInfo.{loaded_context,
  trained_context}` (replacing `context`), `Limits` without `temperature` (it moved into
  `Sampling`), the `embed`, `shape`, `retry` and `sequence` modules, and `From<StopReason> for
  genai_names::Finish`. `InputKind::Audio` is dropped: voice goes through `speech-provider`, and
  there is no `Part::Audio`.
- **`model-catalog`.** Every chat entry writes `sampling = { reasoning_on, reasoning_off }`
  (`SamplingDefaults`); a chat role without it, or a speech entry with it, is refused. `output`
  accepts `gbnf` and `choice`. `holo-3.1-4b` writes proposals (0.6, top_p 0.95, top_k 20 with
  reasoning on; greedy with it off) that a recorded step confirms or corrects.
- **`model-replay`.** The header carries `engine: EngineStamp { kind, build }` where it carried a
  `backend` label (`BackendLabel` stays as a type alias, not a constructor); `Interaction` gains
  `id` and `print: PrintHash`; `ReplayMode::Strict`; `RequestPrint` carries `tool_calls`,
  `sampling` and `engine`; `CassetteError::BadSequence`; the wire cassette types and
  `RecordingTransport`/`ReplayTransport`; `check_sequence`. `CassetteVersion` stays 1: no
  cassette file exists yet, so nothing needs migrating; the first recorded file is the first to
  carry version 1 of this shape.
- **New crates.** `model-extract` and `genai-names` (zero dependencies; `model-provider`
  depends on it for the `Finish` conversion). Boundary rows for all three.
- **Decided.** No untyped JSON escape hatch: sampling is typed and engine knobs are per-flavor
  enum arms. inferd owns retry and structured-output validate-and-repair (porter's
  ARCHITECTURE.md section 7); `ExtractSession` and `Retrying` are the machines it runs.
  `ShapeWithTools` is a `Quirks` field of the flavor and a parameter of `choose`, since
  `Caps` is catalog data.

Left to fill, with the reason (the type is frozen, the spelling is not):

- **Engine constraint syntax.** The parameter names for llama-server's grammar and
  `response_format`, vLLM's structured-output parameter and its backends (`GuidedBackend`; the
  spelling changed across vLLM versions), and Ollama's `format` are not verified: they need the
  engines' current docs or a recorded request. `OutputShape` and `EngineExtras` hold the intent.
- **OpenTelemetry attribute names.** `genai-names` copies the names the research read from rig and
  the convention; the conventions are development-status upstream and have been renamed (`gen_ai.system`
  to `gen_ai.provider.name`). They are checked against the registry when the span macros are
  written. Names are values here, so no stub guards them.
- **`Flavor::quirks()` rows marked "to verify".** Whether `response_format` suppresses tool calls on
  llama-server and vLLM (`shape_with_tools`) is a guess (the conservative `AfterResult` for
  llama-server, `Together` for vLLM); the first recorded fixture decides.
- **The catalog has no embedding table.** `EmbedCaps` exists in `model-provider::embed` (dims, max
  batch, max input, prompts), but a catalog entry for an embedding model still carries only the
  chat `Caps`; the table joins with the first embedding entry, together with the width the
  catalog states for engines that ignore a `dimensions` field.
- **Batching `Ollama` and Anthropic.** `model-ollama` (NDJSON, `EngineExtras::Ollama`) and
  `model-anthropic` (signed thoughts) are later crates; the seams they need (`Framing::Ndjson`,
  `NdjsonDecoder`, `ThoughtSeal`) are frozen here.
- **Rig attribution.** No rig code is copied. Porting its SSE framer tests or its tool-call
  assembly in the fill puts the attribution header of `research-rig.md` in that file and one
  `THIRD-PARTY-NOTICES` entry in the repo.

## Fill F1: stoker-shape (2026-10-03)

Filled: `model-provider` (`shape`, `retry`, `sequence`, `embed`), `model-extract` (`choose`,
`request`, `absorb_for`), `model-http` (`SseDecoder`, `NdjsonDecoder`), `model-openai-compat`
(`StreamDecoder`, the tool-call assembler in `assemble.rs`, the error envelope reader in
`envelope.rs`). Not touched: HTTP io, `Driver`, the request encoder, `classify`, `describe`,
`parse_models`, the embedding codec and the audio wire (F2).

Engine constraint spellings, verified from current docs on 2026-10-03:

| Fact | Source |
| --- | --- |
| llama.cpp GBNF supports `{m,n}`, `{m}`, `{m,}`, `{0,n}`, character classes with `\xXX`, `\uXXXX`, `\UXXXXXXXX` escapes and `^` negation, `#` comments; `x? x? x?` chains sample very slowly, use `x{0,N}` | llama.cpp `grammars/README.md` |
| llama-server takes `grammar` (GBNF), `json_schema`, and `response_format` (plain JSON or schema constrained); `cache_prompt`, `id_slot`; tool calling needs `--jinja`; the docs do not say whether `response_format` suppresses tool calls, so `shape_with_tools = AfterResult` for llama-server stays a conservative guess | llama.cpp `tools/server/README.md` |
| vLLM current spelling: `response_format: {type: "json_schema"}` or `extra_body.structured_outputs` with `choice`, `regex`, `json`, `grammar`, `structural_tag`; backend chosen at serve time with `--structured-outputs-config.backend`. The `guided_json`, `guided_regex`, `guided_choice`, `guided_grammar` and `guided_decoding_backend` spellings are removed from v0.12.0 | docs.vllm.ai `features/structured_outputs` |
| Ollama native: the schema goes in `format`; the OpenAI-compatible endpoint takes `response_format` | docs.ollama.com `capabilities/structured-outputs` |
| Anthropic structured output refuses `minimum`, `maximum`, `multipleOf`, `minLength`, `maxLength`, `maxItems` (and `minItems` beyond 0 or 1), and needs `additionalProperties: false` on every object; the SDKs strip the bounds and move them into descriptions | platform.claude.com structured-outputs |

The `SchemaDialect::Anthropic` sanitiser therefore strips those keywords (the frozen doc comment said
"numeric bounds"); `Shape::check` still enforces every bound after the reply. The request-side
parameter spellings (`structured_outputs` for vLLM, `grammar` and `response_format` for llama-server)
are used by `encode_request` in F2, which keeps the "verify at fill" rows marked in `quirks.rs`
(`shape_with_tools` for llama-server and vLLM) until a recorded fixture settles them. `GuidedBackend`
is a serve-time flag in the current vLLM, not a request parameter; F2 decides whether `VllmExtras`
still has a request field to carry.

Decisions taken in the bodies:

- **`Shape::to_gbnf` describes the canonical text**: fields in declared order, every field present
  (`Optional` is `null` or a value), bounds above 256 left open (llama.cpp unrolls `{m,n}`) and
  enforced by `check`. A reply that follows it always passes `check`; the converse needs the
  canonical form (the property test generates it). `Date`, `DateTime` and `Tagged` stay
  `NotRepresentable`, as the frozen doc comment says.
- **`Shape::to_regex` is the regex of the bare value** (`allow`, `7`), for vLLM's
  `structured_outputs.regex`, which constrains the whole reply. `ExtractSession` reads a bare
  `Choice` or `Regex` reply as the JSON of its shape.
- **`Shape::check` faults** name the nearest enclosing field (`root` at the top) and a kind. An
  unknown key is reported against the field that holds it, not by its own name: the key is the
  model's text.
- **`sanitize_schema` is public** (additive): tool parameter schemas need the same dialects.
- **`sequence::check`** treats a tool call left unanswered at the end of the list as
  `UnansweredCall`: a request never ends there. A tool result inside a user message answers a call.
- **`Retrying`** has no jitter source (the frozen struct holds none): it passes `Permille(0)`.
  `Sleeper` implementations spread callers out if they need to. `Retry-After` is the larger of the
  server's seconds and the backoff, and is not capped.
- **`choose`** applies `ShapeWithTools` to rules 1 and 2 alike (a constraint over the whole reply
  suppresses tool calls wherever it is not `Together`), and reads rule 1 as the scalar shapes
  (`Choice`, `Integer`); a grammar also serves any other shape it can say when there is no schema.
- **`StreamDecoder`**: a call opens on its name (id minted as `call_<n>` when the wire gave none),
  `finish_reason: length` drops a call whose arguments are not valid JSON and keeps whole ones,
  `stop` after delivered calls is `ToolUse`, a bare `[DONE]` before any chunk is `Unreadable`,
  usage is clamped (`cached <= input`), the first of `reasoning_content` and `reasoning` wins.
  The SSE caps are 1 MiB per line and 8 MiB of data per event (`LineTooLong`); NDJSON 4 MiB per line.

Interface asks (nothing in a frozen signature was changed in F1; all four were applied in F2, see `Fill F2: stoker-driver`):

1. **`ExtractSession::absorb` cannot build `Extracted::Repair(Box<TurnRequest>)`**: it has no base
   request. Replace `absorb(&mut self, end, text, calls)` with `absorb_for(&mut self, base, end,
   text, calls)`, which is written and tested; `absorb` is left as a `todo!()` until then.
2. **An error envelope inside a 200 cannot reach the driver as a `ProviderError`**: `CodecError` is
   `Copy` and carries no fault. `StreamDecoder::fault()` returns it today (and `feed` answers
   `Unreadable`); the trait wants either `ChatDecoder::fault(&self) -> Option<ProviderError>` with a
   default of `None`, or a `CodecError::Envelope` variant plus that accessor.
3. **`Retrying` needs a jitter source** if the callers are to spread out: a defaulted
   `Sleeper::jitter(&self) -> Permille` returning zero would be additive.
4. **`ProviderError::RateLimited(RetrySeconds)` from an envelope has no seconds**: the envelope reader
   answers `RetrySeconds(0)`; the driver replaces it with the head's `Retry-After` when it has one.

## Fill F2: stoker-driver (2026-10-03)

Filled: `model-wire` (`Driver::{describe, turn, embed}`), `model-http` (`Transport for HttpClient`
over hyper), `model-openai-compat` (`encode_request`, `classify`, `describe`, `parse_models`,
`encode_embed`, `decode_embed`), `engine-supervisor::command`. 13 `todo!()` removed, 29 left (the
audio wire, the speech crates, `cua-session`, `cua-vendors`).

### Interface asks closed

| Ask | What changed |
| --- | --- |
| 17 | `budget(want, running, gpu, headroom, now, probe_every)`: an engine last used within `probe_every` of `now` is in a turn, counts against `free` and is never a victim; `step` no longer moves those engines into `used_by_others` itself (starting engines still go there) |
| 18 | `CassetteHeader` gains `context: ContextStamp { loaded, trained }` and `speech: Option<SpeechCaps>` (both written, `null` for none); `ReplayProvider::describe` answers the stamp; `SpeechReplay::describe` answers the header's caps for its direction and falls back to what the calls show when there are none. `CassetteVersion` stays 1: no file exists yet |
| 19 | `HttpError::ReplayMiss` (`{"kind":"replay_miss"}`); `ReplayTransport` answers it instead of `Connect` |
| 20 | `engine-supervisor::command` is filled (below) |
| 21 | SSE and NDJSON recording tests through wire cassettes: `model-replay/tests/wire_framed.rs` (line endings, comments, BOM, split code points, cut streams, a proptest over events and chunkings, replay under every `ChunkPlan`) and `model-openai-compat/tests/driver.rs` (the same cassettes through `Driver`) |
| 28 | `DropReason::BadArgument`: an argument that is there and unusable (empty or control-character text, a key chord that is not one key plus modifiers, an unknown direction or finish status, a wrongly typed argument). `MissingArgument` stays for none given |
| 29 | `Target::Centre` (additive, in `cua-action`): a Qwen or Holo `scroll` that names no point acts at the centre of the frame; it passes through every space mapping unchanged and the executor resolves it against the window. A half-given point is still refused |
| 30 | Already done by `stoker-vision` (the `image_tokens` doc says the factor carries the merge); verified |
| 31 | `check-boundary.sh`: `EXCLUDED` takes a nested path (`cua-parse/fuzz`), reading the crate's name from the first line of `cargo tree` |
| 49 | `ExtractSession::absorb(base, end, text, calls)` replaces the stub (it was `absorb_for`); no `todo!()` is left in the crate |
| 50 | `ChatDecoder::fault(&self) -> Option<ProviderError>`, default `None`; `StreamDecoder` implements it (its inherent `fault` is gone); the driver reports it instead of a generic unreadable reply |
| 51 | `Sleeper::jitter() -> Permille`, default zero; `Retrying` passes it to `next_wait` |
| 52 | The driver replaces `RateLimited(RetrySeconds(0))`, from an envelope in a 200 or from `classify`, with the head's `Retry-After` |
| 53 | Decided: `VllmExtras` has no guided-backend field. `GuidedBackend` is gone; `VllmExtras { priority: Knob<Count> }` carries vLLM's per-request `priority` (it acts only under `--scheduling-policy priority`, which is how a foreground turn gets ahead of a background embedding rebuild). The backend is a serve-time flag for `command` to add when a catalog entry names one |

### The driver

- `turn` encodes, then hands the transport a sink adapter (`sink.rs`). The head decides: a non-2xx
  status or an `Html` body goes to a buffer (64 KiB at most, then the connection is stopped) and
  `classify` reads it; anything else is framed by the framing the exchange named (SSE data,
  NDJSON line, or the whole body) and fed to the decoder, whose events go to the caller's sink. A
  frame the decoder refuses ends the stream with the decoder's `fault` (an envelope in a 200) or
  a generic unreadable error; the connection is stopped.
- `Flow::Stop` stops the connection and ends the turn `Ok` with `StopReason::EndTurn` and the
  usage seen so far. A transport that fails after the turn was complete (a reset after the finish
  reason) does not undo the turn.
- Transport failures: `Connect`, `Tls` and `Broken` are `Unreachable`; `Timeout` is `Timeout`; a
  bare 5xx status is `Server`, other statuses `BadRequest("http_<n>")`; `ReplayMiss` is
  `BadRequest("no recorded exchange")`. A turn that was cut after events is not retried by
  `Retrying` whatever the error, as before.
- `describe` and `embed` read a whole body (16 MiB at most; error bodies 64 KiB) through the same
  head rule. `embed` with no inputs asks nothing; the reply is checked for count and width (a
  width asked for, else the first vector's) before it is returned.
- Event names of an SSE frame are dropped at the driver: the trait's frame is the data. Anthropic's
  `ping` and `error` events (a later crate) will need a frame type with the event name.

### The OpenAI-compatible request

Every field a flavor does not understand is left out, never sent hopefully. Verified from the
engines' docs on 2026-10-03 where F1 recorded it (`grammar`, `response_format`, vLLM's
`structured_outputs`, `cache_prompt`, `id_slot`); the rest are **to verify against a recorded
request**:

- `chat_template_kwargs.enable_thinking` for llama-server and vLLM (on and off); `reasoning_effort`
  for vLLM and LiteLLM and `reasoning: {"effort"}` for OpenRouter when on; nothing when off for
  those two. llama-server ignores the effort.
- `top_k`, `min_p`, `seed` as top-level fields everywhere; `repeat_penalty` for llama-server and
  `repetition_penalty` for the others; `max_tokens` (not `max_completion_tokens`) everywhere.
- `parallel_tool_calls` is sent whenever tools are, for every flavor.
- A JSON schema is `response_format: {type: json_schema, json_schema: {name: "reply", strict:
  true, schema}}` for all flavors; llama-server's `Choice` is a one-rule GBNF grammar; vLLM's
  `Regex`, `Lark` and `Choice` are `structured_outputs`; anything else is `UnsupportedShape`
  (`enforceable(flavor)` lists what each flavor takes). A constraint with tools waits for a tool
  result where the flavor says `AfterResult` and is dropped, not refused, before that.
- Thoughts are not handed back to these servers; an assistant turn with only calls has
  `content: ""`; a tool result's images stay in the tool message on llama-server and go to one user
  message after the tool messages elsewhere (the tool message then carries its text, possibly empty).
- A `ToolSpec::Native` has no form on this wire and is `UnsupportedShape` (`CodecError` has no
  variant for it; the name is close enough until a second wire needs the distinction).
- `describe`: `/props` at the server root for llama-server (the per-slot `n_ctx` is the loaded
  context, the model name its alias or the file name of `model_path`), `/models` elsewhere
  (`max_model_len`, `context_length`, `meta.n_ctx_train`). A size the server does not report is
  `Tokens(0)`: callers use the catalog's context; when only one of the two is reported both take it.
- `classify`: the status class decides (401 and 403 `Unauthorized`, 408 `Timeout`, 429
  `RateLimited` with the head's seconds, 5xx `Server` unless the envelope says not ready or a
  context overflow, anything else the envelope's kind or `BadRequest("http_<n>")`); vLLM's
  `maximum context length is N tokens` message is read for its number only; an HTML page served as
  200 is `Unreadable`.

### `HttpClient`

One connection per exchange, driven beside the request (neither is spawned), so dropping the future
drops the socket. Timeouts are the endpoint's (`WaitMs(0)` is a literal zero: the daemon validates
its settings); the request is the endpoint's base plus the path (or the path alone for
`RouteRoot::Server`), `Accept` by framing, the auth header and the extra headers marked sensitive.
The `hyper` feature is off by default so the codecs reach no HTTP stack; the workspace carries
`tokio`, `hyper`, `hyper-util` and `http-body-util` from quire's pinned block. **Not built:** TLS (a
`Tls` target answers `HttpError::Tls`) and the egress proxy (`Via` answers `Connect`); both arrive
with the first cloud backend and `hyper-rustls`/`rustls`. `OpenAiSpeech` still holds the client
directly and waits for a multipart or raw-bytes POST (`Verb` has `Get` and `PostJson` only).

### `command`

vLLM: `<python> -m vllm.entrypoints.openai.api_server --model <snapshot> --served-model-name <id>
--uds <socket>` then the profile's args; llama-server: `--model` and `--mmproj` from the snapshot
(file names stripped of any directory), `--alias <id>`, `--host <socket>` (llama.cpp takes a Unix
socket when the host ends in `.sock`, to verify), then the profile's args; the speech host:
`--model-dir <snapshot>` then the profile's args; Kokoro: `<python> -m uvicorn api.src.main:app`
then the profile's args (the module path is unverified until spike V-T). `{socket}` is replaced
in every profile arg. The snapshot is `<hf_cache>/models--<org>--<name>/snapshots/<revision>`
(each component stripped to `[A-Za-z0-9._-]`, so a catalog entry cannot name a path outside the
cache); the sandbox reads that repository directory (the snapshot's files are links into its
`blobs/`) and writes only the socket's directory (vLLM's and torch's own caches are spike S1's
to place). `memory_max` is host memory: 4096 MiB plus the weights and overhead figures, a
proposal that spike S1 measures. `HF_HUB_OFFLINE=1` always. A llama-server profile whose weights
are not GGUF gets no `--model` (the engine refuses to start); `parse_entry` does not yet refuse the
pairing, which a later catalog change should.

### Consumers

`porter` and `docket` consume stoker by path (`cua-action`, `model-provider`, `speech-vad`). No call
site of a changed signature exists in either repo (searched for `ExtractSession`, `Sleeper`,
`Retrying`, `CassetteHeader`, `HttpError`, `VllmExtras`, `GuidedBackend`, `ChatDecoder`,
`StreamDecoder`, `ModelInfo`, `loaded_context`, `Target::`; docket's `budget` is its own). Two
things to know: `cua-action::Target` gained a variant, so an exhaustive `match` on it in porter's
executor (none today) needs the `Centre` arm; and porter's own `DropReason` mirror
(`porter-infer/src/cua.rs`) has no `BadArgument` yet, which the `cua-parse` to porter mapping will
need when it is written.

## Open

- **Deviations from models.md that SPEC.md left open**, each forced by the crate order:
  - `MiB` lives in `model-catalog` (the catalog's `VramEstimate` needs it, and the supervisor
    sits above the catalog); `UnitSpec`, `Sandbox` and `command` live in `engine-supervisor`
    for the same reason (models.md put `command` in the catalog).
  - `cua-vendors` depends on `cua-parse` (its `decode` returns `Parsed`) and not on
    `vision-prep` (SPEC.md 1.3 lists the reverse; the table there predates `Parsed` having a
    crate of its own). `model-replay` depends on `vision-prep` for `MediaType`.
  - The catalog's `roles` use `CatalogKind`, stoker's copy of porter's `AiKind`: stoker cannot
    name porter. The slugs are identical; porter's `inferd::bridge` owns the total mapping test.
  - `WeightFiles::Gguf` is adjacently tagged (`{ kind = "gguf", v = { model, mmproj } }`) like every
    other data enum, where models.md's example inlined the fields.
  - `RequestPrint` is its own struct (a `TurnRequest` with image bytes replaced by digest and
    size) rather than a newtype over `TurnRequest`, which holds the bytes it must not store.
  - `DropReason` gains `OutOfFrame`: the session reports a point that maps outside the frame as a
    dropped action, in the same list as the parser's drops.
- Limits chosen where the specs named none, frozen with the types (`cua-action/src/text.rs`):
  `Summary` 1024 chars, `Choice` 128 and at most 8 per ask, `TypedText` 2048 (control characters
  other than newline and tab refused), `FieldName` 64, `ExtractedText` 8192, `WaitMs` 60 000,
  `Repeat` 1..=20. `Wait` is classed `Observe` (no side effect), `Zoom` too. A change is a
  SPEC.md edit.
- `Length::new`/`Point::new`/`Size::new` are the only constructors; the `PhantomData` field is
  private, so a space cannot be forged by a struct literal outside `cua-action`.
- Proposed settings without design/22 rows (the quire agent owns the rows): `ai.engine.*` (six
  keys, see `SupervisorConfig::default`), `ai.cua.history_frames` (3), `ai.cua.repair_attempts`
  (1), `ai.engine.vllm.python`, `ai.engine.llama_server.path`.
- The pinned block now carries `hyper`, `hyper-util`, `http-body-util`, `tokio` and `blake3`;
  the workspace names the first four (verbatim) for `model-http`'s `hyper` feature. It also
  carries `hyper-rustls` and `rustls`, which stay unnamed until the first cloud backend needs TLS.
- `deny.toml` is quire's verbatim (the unused MPL allowance warns) plus the `[graph] exclude` of
  the two excluded speech crates.
- **The excluded crates' `Cargo.lock` and `target/` are untracked** (`.gitignore`), and their
  third-party dependencies (`ort`, `sherpa-onnx`) are commented in their manifests until the
  pinned block carries them; the skeletons build with path dependencies only.
- `pipewire` stays out of stoker: capture and playback are docket's `voiced`, and the boundary
  check refuses an audio device crate in any speech crate.
- Speech `Debug` redaction: `PcmBytes`, `Frame512`, `HeardText`, `SpokenText` and `MultipartBody`
  print a length only; `PcmBytes` zeroes its bytes on drop (`zeroize`, in the pinned block).

## Standing facts

- No ds-core, no porter: a closed set's serde form is its slug.
- `todo!()` is allowed only behind a frozen interface; every such stub is listed above.
- The catalog file is `ModelEntry`'s serde form: every field is written, none defaulted, and the
  capability fields sit at the top level beside `id` and `label`.
- The cassette file is JSON Lines: a header line, then one `Interaction` per line; a wire cassette (`<name>.wire.jsonl`) is the same header and one `WireExchange` per line.
- Nothing here records, downloads or starts anything; the dev scripts that do are run by hand.
