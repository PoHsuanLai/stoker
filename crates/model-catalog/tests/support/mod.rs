//! Builds new-shape catalog files from parts, for the parser, narrowing and slot tests.

#![allow(dead_code)]

pub const SAMPLING: &str = r#"sampling = { reasoning_on = { temperature = 600, top_p = { kind = "off" }, top_k = { kind = "off" }, min_p = { kind = "off" }, repeat_penalty = { kind = "off" }, seed = { kind = "off" } }, reasoning_off = { temperature = 0, top_p = { kind = "off" }, top_k = { kind = "off" }, min_p = { kind = "off" }, repeat_penalty = { kind = "off" }, seed = { kind = "off" } }, reasoning_default = "off" }"#;

pub fn text_out(sampling: bool) -> String {
    let sampling = if sampling {
        format!(", {SAMPLING}")
    } else {
        String::new()
    };
    format!(
        r#"text_out = {{ tools = "native", structured = ["json_schema"], reasoning = "absent", streaming = "present", context = 8192, max_output = 1024{sampling} }}"#
    )
}

pub const IMAGE_IN: &str =
    r#"image_in = { per_prompt = 2, rule = { kind = "identity" }, space = { kind = "image" } }"#;
pub const AUDIO_IN: &str = r#"audio_in = { dir = "in", streaming = "absent", partials = "absent", punctuation = "present", timestamps = "absent", langs = { kind = "any" }, max_audio_ms = 30000, input = { rate = 16000, pcm = "s16_le" } }"#;
pub const AUDIO_OUT: &str = r#"audio_out = { dir = "out", streaming = "present", partials = "absent", punctuation = "absent", timestamps = "absent", langs = { kind = "any" }, max_audio_ms = 60000, output = { rate = 24000, pcm = "s16_le" }, voices = ["a"] }"#;
pub const VECTOR_OUT: &str = r#"vector_out = { dims = 768, max_batch = 32, max_input = 8192, prompts = { query = "q: ", document = "d: " } }"#;
pub const ACTIONS_OUT: &str = r#"actions_out = { kind = "dialect", v = { dialect = { kind = "tool", v = "holo31" }, batching = "one", zoom = "absent" } }"#;
pub const ENGINE: &str = "[[engine]]\nkind = \"llama_server\"\nargs = []\nweights = { kind = \"gguf\", v = { model = \"m.gguf\" } }\n";

/// A file with these inputs, outputs and table lines, and these `[[engine]]` blocks.
pub fn file(id: &str, inputs: &str, outputs: &str, tables: &[String], engines: &str) -> String {
    format!(
        r#"id = "{id}"
label = "{id}"
licence = {{ kind = "open", v = "Apache-2.0" }}
family = "test"
cold_start_estimate_s = 1
source = {{ kind = "hugging_face", v = {{ repo = "a/b", revision = "0123456789abcdef0123456789abcdef01234567" }} }}
vram = {{ weights_mib = 100, kv_per_1k_ctx_mib = 1, overhead_mib = 10 }}
inputs = {inputs}
outputs = {outputs}
{}
{engines}"#,
        tables.join("\n")
    )
}

/// Text, image and audio in; text out.
pub fn multimodal(id: &str, engines: &str) -> String {
    file(
        id,
        r#"["text", "image", "audio"]"#,
        r#"["text"]"#,
        &[text_out(true), IMAGE_IN.into(), AUDIO_IN.into()],
        engines,
    )
}

pub fn text_only(id: &str) -> String {
    file(id, r#"["text"]"#, r#"["text"]"#, &[text_out(true)], ENGINE)
}

/// A named engine block of this kind with no narrowing.
pub fn engine(kind: &str) -> String {
    let weights = match kind {
        "llama_server" => r#"{ kind = "gguf", v = { model = "m.gguf" } }"#,
        _ => r#"{ kind = "hf_snapshot" }"#,
    };
    format!("[[engine]]\nkind = \"{kind}\"\nargs = []\nweights = {weights}\n")
}

/// The same with a narrowing line, such as `inputs = ["text", "image"]`.
pub fn narrowed_engine(kind: &str, narrowing: &str) -> String {
    engine(kind).replace("args = []", &format!("args = []\n{narrowing}"))
}
