use model_http::{AuthHeader, HttpClient, HttpEndpoint, HttpTarget, Proxy, UrlPath};
use model_openai_compat::{Flavor, OpenAiCompat, RequestJson};
use std::path::PathBuf;

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
    });
    assert_eq!(
        OpenAiCompat::new(client, Flavor::Vllm).flavor(),
        Flavor::Vllm
    );
}

#[test]
fn request_bodies_do_not_print() {
    assert_eq!(
        format!("{:?}", RequestJson("{}".into())),
        "RequestJson(<2 bytes>)"
    );
}
