use model_http::RouteRoot;
use model_openai_compat::{
    DimensionsField, Flavor, LogprobsAsk, Quirks, RequestJson, ToolImages, ToolNaming, UsageAsk,
};
use model_provider::ShapeWithTools;

#[test]
fn flavors_have_stable_slugs() {
    use model_openai_compat::SpeechFlavor;
    const CHAT: &[(Flavor, &str)] = &[
        (Flavor::LlamaServer, r#""llama_server""#),
        (Flavor::Vllm, r#""vllm""#),
        (Flavor::LiteLlm, r#""lite_llm""#),
        (Flavor::OpenRouter, r#""open_router""#),
    ];
    for (flavor, json) in CHAT {
        assert_eq!(&serde_json::to_string(flavor).unwrap(), json, "chat {json}");
        assert_eq!(
            &serde_json::from_str::<Flavor>(json).unwrap(),
            flavor,
            "chat {json}"
        );
    }
    const SPEECH: &[(SpeechFlavor, &str)] = &[
        (SpeechFlavor::Vllm, r#""vllm""#),
        (SpeechFlavor::KokoroFastApi, r#""kokoro_fast_api""#),
    ];
    for (flavor, json) in SPEECH {
        assert_eq!(
            &serde_json::to_string(flavor).unwrap(),
            json,
            "speech {json}"
        );
    }
}

#[test]
fn request_bodies_do_not_print() {
    assert_eq!(
        format!("{:?}", RequestJson("{}".into())),
        "RequestJson(<2 bytes>)"
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
