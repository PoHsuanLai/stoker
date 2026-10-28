use model_openai_compat::{Flavor, RequestJson};
use model_provider::ShapeWithTools;

#[test]
fn flavors_have_stable_slugs() {
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
fn what_porter_reads_of_a_server_is_pinned() {
    const CASES: &[(Flavor, ShapeWithTools)] = &[
        (Flavor::LlamaServer, ShapeWithTools::AfterResult),
        (Flavor::Vllm, ShapeWithTools::Together),
        (Flavor::LiteLlm, ShapeWithTools::AfterResult),
        (Flavor::OpenRouter, ShapeWithTools::AfterResult),
    ];
    for (flavor, shape_with_tools) in CASES {
        assert_eq!(
            flavor.quirks().shape_with_tools,
            *shape_with_tools,
            "{flavor:?}"
        );
    }
}
