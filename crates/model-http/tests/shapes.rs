use std::path::PathBuf;

use model_http::{
    AuthHeader, HeaderName, HostName, HttpEndpoint, HttpError, HttpStatus, HttpTarget, JsonBody,
    Port, Proxy, Secret, SseDecoder, UrlPath,
};

#[test]
fn endpoint_round_trips_with_pinned_json() {
    let endpoint = HttpEndpoint {
        target: HttpTarget::Unix(PathBuf::from("/run/user/1000/inferd/vllm.sock")),
        proxy: Proxy::Direct,
        base: UrlPath("/v1".into()),
        auth: AuthHeader::None,
    };
    let json = serde_json::to_string(&endpoint).unwrap();
    assert_eq!(
        json,
        r#"{"target":{"kind":"unix","v":"/run/user/1000/inferd/vllm.sock"},"proxy":{"kind":"direct"},"base":"/v1","auth":{"kind":"none"}}"#
    );
    assert_eq!(
        serde_json::from_str::<HttpEndpoint>(&json).unwrap(),
        endpoint
    );
}

#[test]
fn a_proxied_cloud_target_round_trips() {
    let endpoint = HttpEndpoint {
        target: HttpTarget::Tls {
            host: HostName("api.example.com".into()),
            port: Port(443),
        },
        proxy: Proxy::Via(Box::new(HttpTarget::Unix(PathBuf::from(
            "/run/egress.sock",
        )))),
        base: UrlPath("/v1".into()),
        auth: AuthHeader::Header {
            name: HeaderName("x-api-key".into()),
            value: Secret("k".into()),
        },
    };
    let json = serde_json::to_string(&endpoint).unwrap();
    assert_eq!(
        serde_json::from_str::<HttpEndpoint>(&json).unwrap(),
        endpoint
    );
}

#[test]
fn secrets_and_bodies_do_not_print() {
    let auth = AuthHeader::Bearer(Secret("sk-very-secret".into()));
    assert!(!format!("{auth:?}").contains("sk-very-secret"));
    assert_eq!(
        format!("{:?}", JsonBody("{\"a\":1}".into())),
        "JsonBody(<7 bytes>)"
    );
}

#[test]
fn errors_round_trip() {
    let error = HttpError::Status(HttpStatus(503));
    let json = serde_json::to_string(&error).unwrap();
    assert_eq!(json, r#"{"kind":"status","v":503}"#);
    assert_eq!(serde_json::from_str::<HttpError>(&json).unwrap(), error);
}

#[test]
fn a_new_decoder_is_empty_and_clonable() {
    let decoder = SseDecoder::new();
    let copy = decoder.clone();
    assert_eq!(format!("{decoder:?}"), format!("{copy:?}"));
}
