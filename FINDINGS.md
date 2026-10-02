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

## Stubs behind frozen interfaces

Every `todo!()` in the repo (31). Each is a signature other repos build on; the body arrives with
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

### `model-http` (4)

- `src/client.rs`: HttpClient::get over hyper
- `src/client.rs`: HttpClient::post_json over hyper
- `src/sse.rs`: SseDecoder::feed: lines, fields, blank line dispatches
- `src/sse.rs`: SseDecoder::finish

Closes when hyper, hyper-util and http-body-util join the pinned block, then the decoder passes `sse_decoder_any_chunking` and the client a loopback Unix-socket test.

### `model-openai-compat` (5)

- `src/codec.rs`: encode_request: messages, images, tools, constraints, stream_options
- `src/codec.rs`: StreamDecoder::feed: deltas, reasoning, tool-call fragments, usage
- `src/codec.rs`: StreamDecoder::finish: stop reason and usage
- `src/provider.rs`: OpenAiCompat::describe: GET /models
- `src/provider.rs`: OpenAiCompat::turn: encode, post, decode the stream into the sink

Closes when `encode_request` passes its golden request and `StreamDecoder` the recorded SSE fixtures from vLLM and llama-server (`dev/record-engine.sh`, run by hand).

### `model-replay` (4)

- `src/print.rs`: RequestPrint::of: blake3 over image bytes
- `src/provider.rs`: ReplayProvider::describe: the cassette's model
- `src/provider.rs`: ReplayProvider::turn: match, push events, return the end
- `src/provider.rs`: RecordingProvider::turn: tee events into an Interaction, write it

Closes when `blake3` joins quire `docs/workspace-deps.toml`, then the providers replay and record against a cassette.

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
- `deny.toml` is quire's verbatim (the unused MPL allowance warns).

## Standing facts

- No ds-core, no porter: a closed set's serde form is its slug.
- `todo!()` is allowed only behind a frozen interface; every such stub is listed above.
- The catalog file is `ModelEntry`'s serde form: every field is written, none defaulted, and the
  capability fields sit at the top level beside `id` and `label`.
- The cassette file is JSON Lines: a header line, then one `Interaction` per line.
- Nothing here records, downloads or starts anything; the dev scripts that do are run by hand.
