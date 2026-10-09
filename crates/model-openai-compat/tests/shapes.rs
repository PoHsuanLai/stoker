use model_http::{
    AuthHeader, HttpClient, HttpEndpoint, HttpTarget, Proxy, RouteRoot, Timeouts, UrlPath, WaitMs,
};
use model_openai_compat::{
    DimensionsField, Flavor, LogprobsAsk, OpenAiCodec, OpenAiCompat, Quirks, RequestJson,
    ToolImages, ToolNaming, UsageAsk,
};
use model_provider::ShapeWithTools;
use model_wire::ChatCodec;
use std::path::PathBuf;

fn timeouts() -> Timeouts {
    Timeouts {
        connect: WaitMs(2_000),
        first_byte: WaitMs(60_000),
        idle: WaitMs(30_000),
    }
}

#[test]
fn flavors_have_stable_slugs() {
    const CASES: &[(Flavor, &str)] = &[
        (Flavor::LlamaServer, r#""llama_server""#),
        (Flavor::Vllm, r#""vllm""#),
        (Flavor::LiteLlm, r#""lite_llm""#),
        (Flavor::OpenRouter, r#""open_router""#),
    ];
    for (flavor, json) in CASES {
        assert_eq!(&serde_json::to_string(flavor).unwrap(), json);
        assert_eq!(&serde_json::from_str::<Flavor>(json).unwrap(), flavor);
    }
}

#[test]
fn a_provider_is_built_from_a_client_and_a_flavor() {
    let client = HttpClient::new(HttpEndpoint {
        target: HttpTarget::Unix(PathBuf::from("/run/inferd/vllm.sock")),
        proxy: Proxy::Direct,
        base: UrlPath("/v1".into()),
        auth: AuthHeader::None,
        headers: vec![],
        timeouts: timeouts(),
    });
    let provider: OpenAiCompat = OpenAiCodec::new(Flavor::Vllm).provider(client);
    assert_eq!(provider.codec().flavor(), Flavor::Vllm);
}

#[test]
fn request_bodies_do_not_print() {
    assert_eq!(
        format!("{:?}", RequestJson("{}".into())),
        "RequestJson(<2 bytes>)"
    );
}

#[test]
fn speech_flavors_have_stable_slugs() {
    use model_openai_compat::SpeechFlavor;
    assert_eq!(
        serde_json::to_string(&SpeechFlavor::Vllm).unwrap(),
        r#""vllm""#
    );
    assert_eq!(
        serde_json::to_string(&SpeechFlavor::KokoroFastApi).unwrap(),
        r#""kokoro_fast_api""#
    );
}

#[test]
fn a_speech_provider_is_built_from_a_client_and_a_flavor() {
    use model_openai_compat::{OpenAiSpeech, SpeechFlavor};
    let client = HttpClient::new(HttpEndpoint {
        target: HttpTarget::Unix(PathBuf::from("/run/inferd/kokoro.sock")),
        proxy: Proxy::Direct,
        base: UrlPath("/v1".into()),
        auth: AuthHeader::None,
        headers: vec![],
        timeouts: timeouts(),
    });
    assert_eq!(
        OpenAiSpeech::new(client, SpeechFlavor::KokoroFastApi).flavor(),
        SpeechFlavor::KokoroFastApi
    );
}

#[test]
fn multipart_bodies_do_not_print_the_audio() {
    use model_openai_compat::MultipartBody;
    let body = MultipartBody {
        content_type: "multipart/form-data; boundary=x".into(),
        bytes: vec![1, 2, 3],
    };
    assert_eq!(format!("{body:?}"), "MultipartBody(<3 bytes>)");
}

#[test]
fn the_quirk_table_is_pinned() {
    const CASES: &[(Flavor, Quirks)] = &[
        (
            Flavor::LlamaServer,
            Quirks {
                usage: UsageAsk::Request,
                tool_naming: ToolNaming::AutoOnly,
                tool_images: ToolImages::InToolMessage,
                shape_with_tools: ShapeWithTools::AfterResult,
                describe_root: RouteRoot::Server,
                dimensions: DimensionsField::Ignored,
                logprobs: LogprobsAsk::Request,
            },
        ),
        (
            Flavor::Vllm,
            Quirks {
                usage: UsageAsk::Request,
                tool_naming: ToolNaming::Any,
                tool_images: ToolImages::NextUserMessage,
                shape_with_tools: ShapeWithTools::Together,
                describe_root: RouteRoot::Base,
                dimensions: DimensionsField::Send,
                logprobs: LogprobsAsk::Request,
            },
        ),
        (
            Flavor::LiteLlm,
            Quirks {
                usage: UsageAsk::Request,
                tool_naming: ToolNaming::Any,
                tool_images: ToolImages::NextUserMessage,
                shape_with_tools: ShapeWithTools::AfterResult,
                describe_root: RouteRoot::Base,
                dimensions: DimensionsField::Send,
                logprobs: LogprobsAsk::Unsupported,
            },
        ),
        (
            Flavor::OpenRouter,
            Quirks {
                usage: UsageAsk::Never,
                tool_naming: ToolNaming::Any,
                tool_images: ToolImages::NextUserMessage,
                shape_with_tools: ShapeWithTools::AfterResult,
                describe_root: RouteRoot::Base,
                dimensions: DimensionsField::Send,
                logprobs: LogprobsAsk::Unsupported,
            },
        ),
    ];
    for (flavor, quirks) in CASES {
        assert_eq!(flavor.quirks(), *quirks, "{flavor:?}");
        let json = serde_json::to_string(quirks).unwrap();
        assert_eq!(serde_json::from_str::<Quirks>(&json).unwrap(), *quirks);
    }
}

#[test]
fn a_quirk_row_has_pinned_json() {
    assert_eq!(
        serde_json::to_string(&Flavor::LlamaServer.quirks()).unwrap(),
        r#"{"usage":"request","tool_naming":"auto_only","tool_images":"in_tool_message","shape_with_tools":"after_result","describe_root":"server","dimensions":"ignored","logprobs":"request"}"#
    );
}

#[test]
fn only_llama_server_refuses_a_named_tool_and_ignores_dimensions() {
    for flavor in [
        Flavor::LlamaServer,
        Flavor::Vllm,
        Flavor::LiteLlm,
        Flavor::OpenRouter,
    ] {
        let q = flavor.quirks();
        assert_eq!(
            q.tool_naming == ToolNaming::AutoOnly,
            flavor == Flavor::LlamaServer
        );
        assert_eq!(
            q.dimensions == DimensionsField::Ignored,
            flavor == Flavor::LlamaServer
        );
    }
}

#[test]
fn a_codec_hands_out_a_decoder_for_the_served_model() {
    use model_provider::ModelName;
    let codec = OpenAiCodec::new(Flavor::LlamaServer);
    // The decoder is plain data until a frame arrives.
    let decoder = codec.decoder(ModelName("holo".into()));
    assert!(format!("{decoder:?}").contains("LlamaServer"));
}
