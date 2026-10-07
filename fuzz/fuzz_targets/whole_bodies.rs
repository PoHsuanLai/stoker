//! Bodies read whole: model lists, embeddings, transcriptions and error replies.
#![no_main]

use libfuzzer_sys::fuzz_target;
use model_http::{BodyKind, HttpStatus, ResponseHead};
use model_openai_compat::{Flavor, OpenAiCodec, decode_transcription};
use model_provider::ModelName;
use model_wire::{ChatCodec, EmbedCodec, ErrorWire};

fuzz_target!(|data: &[u8]| {
    for flavor in [Flavor::LlamaServer, Flavor::Vllm] {
        let codec = OpenAiCodec::new(flavor);
        let _ = codec.parse_models(data);
        if let Ok(end) = codec.decode_embed(ModelName("m".into()), data) {
            assert!(
                end.vectors
                    .iter()
                    .all(|v| v.0.iter().all(|f| f.is_finite()))
            );
        }
        for status in [200, 400, 429, 500] {
            for body in [BodyKind::Json, BodyKind::Html] {
                let head = ResponseHead {
                    status: HttpStatus(status),
                    body,
                    retry_after: None,
                    request_id: None,
                };
                let _ = codec.classify(&head, data);
            }
        }
    }
    let _ = decode_transcription(data);
});
