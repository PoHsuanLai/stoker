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

Every `todo!()` in the repo (91: 86 in the workspace, 5 in the two excluded crates; the rig amendment added 27, see `The rig amendment` below). Each is a signature other repos build on; the body arrives with
the work in the "Closes when" line of its crate.

### `cua-parse` (2)

- `src/parse.rs`: parse_text: UiTars15 grammar
- `src/parse.rs`: parse_tool_calls: QwenComputerUse and Holo31 schemas

Closes when the UiTars15 grammar parses the model-card examples; Holo31 and QwenComputerUse parse calls recorded by `dev/record-engine.sh`; `never_panics` (proptest) and the `fuzz/` targets pass.

### `cua-session` (2)

- `src/session.rs`: CuaSession::request: system prompt, history window, observation, frame
- `src/session.rs`: CuaSession::absorb: parse, one repair, map, history

Closes when `request` and `absorb` pass the ScriptedProvider tests (history keeps the last N frames, one repair then unparseable, out-of-frame becomes dropped); prompt files in `prompts/` carry their model-card attribution.

### `cua-vendors` (3)

- `src/codec.rs`: WireCodecs::tools: one declaration per vendor
- `src/codec.rs`: WireCodecs::decode: one decoder per vendor
- `src/codec.rs`: WireCodecs::results: stop-at-first-failure text per vendor

Closes when the first cloud backend is turned on (the user chose open models only).

### `engine-supervisor` (3)

- `src/budget.rs`: budget: fit, LRU eviction of idle engines, NoRoom with numbers
- `src/step.rs`: step: the engine lifecycle table
- `src/unit.rs`: command: program, args from the profile, socket flag, HF_HUB_OFFLINE, sandbox

Closes when `step` passes the table of every row of models.md 4.1, `budget` its table (Fits, LRU eviction, never an active engine, NoRoom with numbers), `command` its table for Holo under vLLM.

### `model-http` (5)

- `src/exchange.rs`: `Transport for HttpClient`: exchange over hyper (Tcp, Unix or Tls by target, proxy)
- `src/sse.rs`: SseDecoder::feed: lines, fields, blank line dispatches
- `src/sse.rs`: SseDecoder::finish
- `src/ndjson.rs`: NdjsonDecoder::feed: split on newline, skip blank lines, cap the line
- `src/ndjson.rs`: NdjsonDecoder::finish

Closes when hyper, hyper-util and http-body-util join the pinned block, then the decoders pass `sse_decoder_any_chunking` (and its NDJSON twin) and the transport a loopback Unix-socket test that checks the head (status, `Content-Type` kind, `Retry-After`, request id) arrives before the first chunk and that a non-success answer is `Rejected` with its body in the sink. The SSE edge cases to port are the list in `research-rig.md` section 3.1 (CR, LF and CRLF split across chunks, a BOM split across chunks, comment lines, an event reset by a blank line, a multibyte code point split across chunks, a line cap and a per-event data cap, a truncated trailing event never delivered).

### `model-openai-compat` (17)

- `src/codec.rs`: encode_request: messages, images, tools, constraints, sampling, engine extras, stream_options
- `src/codec.rs`: OpenAiCodec::classify: status classes, the error envelope, an HTML 200
- `src/codec.rs`: OpenAiCodec::encode: POST /chat/completions, Framing::Sse
- `src/codec.rs`: OpenAiCodec::describe: GET /models, or /props at the server root for llama-server
- `src/codec.rs`: OpenAiCodec::parse_models: loaded and trained context per flavor
- `src/codec.rs`: OpenAiCodec::encode_embed: POST /embeddings, no `dimensions` where the quirk says Ignored
- `src/codec.rs`: OpenAiCodec::decode_embed: data[].embedding in index order
- `src/codec.rs`: StreamDecoder::feed: deltas, reasoning, tool-call fragments, usage
- `src/codec.rs`: StreamDecoder::finish: stop reason and usage
- `src/audio/codec.rs`: encode_speech_request: model, input, voice, response_format pcm, stream true
- `src/audio/codec.rs`: encode_transcription: WAV header over the samples, form fields, boundary
- `src/audio/codec.rs`: decode_transcription: the `text` field of the JSON
- `src/audio/codec.rs`: PcmDecoder::feed: join the carry, keep the partial sample, number the chunk
- `src/audio/provider.rs`: OpenAiSpeech::describe (in): GET /models, caps from the catalog entry
- `src/audio/provider.rs`: OpenAiSpeech::transcribe: drain the audio, POST multipart, one Final event
- `src/audio/provider.rs`: OpenAiSpeech::describe (out): GET /models and /audio/voices
- `src/audio/provider.rs`: OpenAiSpeech::speak: POST /audio/speech, PcmDecoder into the sink

Closes when `encode_request` passes its golden request and `StreamDecoder` the recorded wire fixtures from vLLM and llama-server (`dev/record-engine.sh`, run by hand, recorded at the `Transport`) and the conformance list in `research-rig.md` sections 3.2, 3.3 and 3.7 (the quirk table, tool-call delta assembly with its four `IfMalformed` outcomes, an index reused for a second call, a call delivered in one chunk, an empty arguments string meaning `{}`, an error envelope inside a 200, a bare `[DONE]` that fabricates nothing, every chunk split offset); `Flavor::quirks` is a pinned table whose rows marked "to verify" in `quirks.rs` are settled by the first recorded fixture of that engine; the `audio` bodies close with `model-http` gaining a multipart or raw-bytes POST (`post_json` cannot carry a WAV file), then `encode_speech_request` and `encode_transcription` pass golden bodies, `PcmDecoder` its chunking proptest, and `OpenAiSpeech` a loopback test against fixtures recorded by `dev/record-speech.sh` (by hand, against the user's own vLLM and Kokoro).

### `model-replay` (18)

- `src/print.rs`: RequestPrint::of: blake3 over image bytes
- `src/print.rs`: RequestPrint::hash: canonical JSON, blake3
- `src/cassette.rs`: Cassette::check_sequences: check_sequence per interaction, then `from_jsonl` calls it
- `src/sequence.rs`: check_sequence: the per-index state of every tool call
- `src/wire.rs`: RecordingTransport::exchange: tee head and frames into a WireExchange, scrub, write
- `src/wire.rs`: ReplayTransport::exchange: match the request, deliver head then chunks, `WireEnd` as the error
- `src/provider.rs`: ReplayProvider::describe: the cassette's model
- `src/provider.rs`: ReplayProvider::turn: match, push events, return the end
- `src/provider.rs`: RecordingProvider::turn: tee events into an Interaction, write it
- `src/speech.rs`: AudioPrint::of: blake3 over the samples, summed length and duration
- `src/speech.rs`: SttPrint::of: the request's fields and AudioPrint::of the chunks
- `src/speech.rs`: TtsPrint::of: blake3 over the text
- `src/speech.rs`: SpeechReplay::describe (two impls): the cassette's model
- `src/speech.rs`: SpeechReplay::transcribe: drain the audio, match, push events, return the end
- `src/speech.rs`: SpeechReplay::speak: match, push silent chunks of the recorded duration
- `src/speech.rs`: RecordingSpeech::transcribe: tee audio and events into an interaction
- `src/speech.rs`: RecordingSpeech::speak: tee the audio into a print, write the interaction

Closes when `blake3` joins quire `docs/workspace-deps.toml`, then the providers and transports replay and record against a cassette (chat and speech alike; a speech replay answers silence of the recorded duration, and no audio is ever in a cassette).

### `model-wire` (3)

- `src/driver.rs`: Driver::describe: exchange codec.describe(), parse_models
- `src/driver.rs`: Driver::turn: encode, exchange, classify on a bad head, framer into decoder into the sink
- `src/driver.rs`: Driver::embed: encode_embed, exchange, decode_embed, the per-reply count and width check

Closes with `model-http`'s transport and decoders: the sink adapter buffers a non-success or `Html` body (64 KiB at most) and maps it with `classify`; otherwise the framer the exchange named feeds `ChatDecoder`; `Flow::Stop` answers `ChunkFlow::Stop`; a scripted `Transport` drives it in tests.

### `model-provider` (11)

- `src/embed.rs`: EmbedEnd::check: the count, then every width
- `src/embed.rs`: plan_batches: chunks of max(1, max), covering 0..n exactly
- `src/sequence.rs`: check: port of rig's validate_canonical
- `src/retry.rs`: ProviderError::retry_class: rig's retryable_status and transient_transport, ported
- `src/retry.rs`: next_wait: exponent clamped at 31, cap, Overload takes the larger of server and backoff
- `src/retry.rs`: Retrying::describe and Retrying::turn (2)
- `src/shape.rs`: Shape::to_json_schema, to_gbnf, to_regex, check (4)

Closes when `plan_batches` passes a table and a property (the ranges cover 0..n exactly, in order, none over the cap), `check` the rig scenarios (consecutive assistant, unanswered call, orphan result), `next_wait` a table with an injected jitter and a fake `Sleeper`, `Retrying` a test that never retries after the first event, and the `Shape` conversions the round-trip property of `research-rig.md` section 6d.5 (any value accepted by `check` is accepted by the generated GBNF, through a tiny matcher in tests only).

### `model-extract` (3)

- `src/mode.rs`: choose: the four rules, in order
- `src/session.rs`: ExtractSession::request: output, synthetic tool, tool_choice, limits
- `src/session.rs`: ExtractSession::absorb: Truncated before anything, then T::read, then one repair

Closes when the seven tests of `research-rig.md` section 6d.5 pass (`choose_picks_native_choice_on_llama_server`, `choose_falls_back_to_tool_when_schema_and_tools_conflict`, `repair_message_never_echoes_output`, `one_repair_then_failed`, `max_tokens_is_failed_not_repaired`, `shape_to_gbnf_roundtrip_checks`, `named_tool_refused_on_llama_server`).

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

### `vision-prep` (8)

- `src/frame_map.rs`: FrameMap::new: device size from scale, then fit
- `src/frame_map.rs`: image_to_window: round half up in u64, refuse outside
- `src/frame_map.rs`: grid_to_window: divisor is the grid max
- `src/frame_map.rs`: window_to_image
- `src/frame_map.rs`: length_to_window
- `src/pixels.rs`: prepare: fast_image_resize then image encode
- `src/rule.rs`: fit: smart_resize, long-edge and identity rules
- `src/rule.rs`: image_tokens: per-rule token estimate

Closes when `fit` matches the reference `smart_resize` on `fixtures/smart_resize.csv` (made by `dev/smart-resize-vectors.py` from qwen-vl-utils); the map passes its tables and the proptest; `prepare` resizes once and encodes once.

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
- The pinned block does not yet carry `hyper`, `hyper-util`, `http-body-util` or `blake3`
  (SPEC.md 1.4 adopts them); `model-http` and `model-replay` name none of them before it does,
  so their bodies stay stubbed.
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
