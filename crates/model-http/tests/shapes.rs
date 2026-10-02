use std::path::PathBuf;

use model_http::{
    AuthHeader, BodyKind, ChunkFlow, Exchange, ExtraHeader, Framing, HeaderName, HostName,
    HttpEndpoint, HttpError, HttpStatus, HttpTarget, JsonBody, NdjsonDecoder, Port, Proxy,
    RequestId, ResponseHead, RouteRoot, Secret, SseDecoder, Timeouts, UrlPath, Verb, WaitMs,
    WaitSeconds,
};

fn timeouts() -> Timeouts {
    Timeouts {
        connect: WaitMs(2_000),
        first_byte: WaitMs(60_000),
        idle: WaitMs(30_000),
    }
}

#[test]
fn endpoint_round_trips_with_pinned_json() {
    let endpoint = HttpEndpoint {
        target: HttpTarget::Unix(PathBuf::from("/run/user/1000/inferd/vllm.sock")),
        proxy: Proxy::Direct,
        base: UrlPath("/v1".into()),
        auth: AuthHeader::None,
        headers: vec![],
        timeouts: timeouts(),
    };
    let json = serde_json::to_string(&endpoint).unwrap();
    assert_eq!(
        json,
        r#"{"target":{"kind":"unix","v":"/run/user/1000/inferd/vllm.sock"},"proxy":{"kind":"direct"},"base":"/v1","auth":{"kind":"none"},"headers":[],"timeouts":{"connect":2000,"first_byte":60000,"idle":30000}}"#
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
        headers: vec![ExtraHeader {
            name: HeaderName("anthropic-version".into()),
            value: Secret("2023-06-01".into()),
        }],
        timeouts: timeouts(),
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

#[test]
fn rejected_is_a_unit_error_and_round_trips() {
    let json = serde_json::to_string(&HttpError::Rejected).unwrap();
    assert_eq!(json, r#"{"kind":"rejected"}"#);
    assert_eq!(
        serde_json::from_str::<HttpError>(&json).unwrap(),
        HttpError::Rejected
    );
}

#[test]
fn a_response_head_round_trips_with_pinned_json() {
    let head = ResponseHead {
        status: HttpStatus(429),
        body: BodyKind::Json,
        retry_after: Some(WaitSeconds(7)),
        request_id: Some(RequestId::new("req_01HX").unwrap()),
    };
    let json = serde_json::to_string(&head).unwrap();
    assert_eq!(
        json,
        r#"{"status":429,"body":"json","retry_after":7,"request_id":"req_01HX"}"#
    );
    assert_eq!(serde_json::from_str::<ResponseHead>(&json).unwrap(), head);
}

#[test]
fn body_kinds_framings_verbs_and_roots_have_stable_slugs() {
    const BODY: &[(BodyKind, &str)] = &[
        (BodyKind::EventStream, r#""event_stream""#),
        (BodyKind::Json, r#""json""#),
        (BodyKind::NdJson, r#""nd_json""#),
        (BodyKind::Html, r#""html""#),
        (BodyKind::Other, r#""other""#),
    ];
    for (kind, json) in BODY {
        assert_eq!(&serde_json::to_string(kind).unwrap(), json);
        assert_eq!(&serde_json::from_str::<BodyKind>(json).unwrap(), kind);
    }
    const FRAMING: &[(Framing, &str)] = &[
        (Framing::Sse, r#""sse""#),
        (Framing::Ndjson, r#""ndjson""#),
        (Framing::Whole, r#""whole""#),
    ];
    for (framing, json) in FRAMING {
        assert_eq!(&serde_json::to_string(framing).unwrap(), json);
        assert_eq!(&serde_json::from_str::<Framing>(json).unwrap(), framing);
    }
    assert_eq!(
        serde_json::to_string(&Verb::PostJson).unwrap(),
        r#""post_json""#
    );
    assert_eq!(
        serde_json::to_string(&RouteRoot::Server).unwrap(),
        r#""server""#
    );
    assert_eq!(serde_json::from_str::<Verb>(r#""get""#).unwrap(), Verb::Get);
    assert_eq!(
        serde_json::from_str::<RouteRoot>(r#""base""#).unwrap(),
        RouteRoot::Base
    );
}

#[test]
fn a_request_id_is_checked_where_it_enters() {
    assert!(RequestId::new("req_01HX").is_ok());
    assert!(RequestId::new("").is_err());
    assert!(RequestId::new("a b").is_err());
    assert!(RequestId::new("x".repeat(129)).is_err());
    assert!(serde_json::from_str::<RequestId>(r#""with space""#).is_err());
}

#[test]
fn timeouts_round_trip() {
    let json = serde_json::to_string(&timeouts()).unwrap();
    assert_eq!(json, r#"{"connect":2000,"first_byte":60000,"idle":30000}"#);
    assert_eq!(serde_json::from_str::<Timeouts>(&json).unwrap(), timeouts());
}

#[test]
fn extra_header_values_do_not_print() {
    let header = ExtraHeader {
        name: HeaderName("x-routing".into()),
        value: Secret("tenant-secret".into()),
    };
    assert!(!format!("{header:?}").contains("tenant-secret"));
    let json = serde_json::to_string(&header).unwrap();
    assert_eq!(serde_json::from_str::<ExtraHeader>(&json).unwrap(), header);
}

#[test]
fn an_exchange_does_not_print_its_body() {
    let ex = Exchange {
        verb: Verb::PostJson,
        root: RouteRoot::Base,
        path: UrlPath("/chat/completions".into()),
        body: Some(JsonBody(r#"{"prompt":"private"}"#.into())),
        framing: Framing::Sse,
    };
    assert!(!format!("{ex:?}").contains("private"));
    assert_eq!(ex.clone(), ex);
}

#[test]
fn a_sink_sees_the_head_before_a_chunk() {
    use model_http::BodySink;
    struct Order(Vec<&'static str>);
    impl BodySink for Order {
        fn head(&mut self, _: &ResponseHead) -> ChunkFlow {
            self.0.push("head");
            ChunkFlow::Continue
        }
        fn chunk(&mut self, _: &[u8]) -> ChunkFlow {
            self.0.push("chunk");
            ChunkFlow::Stop
        }
    }
    let mut sink = Order(vec![]);
    let head = ResponseHead {
        status: HttpStatus(200),
        body: BodyKind::EventStream,
        retry_after: None,
        request_id: None,
    };
    assert_eq!(sink.head(&head), ChunkFlow::Continue);
    assert_eq!(sink.chunk(b"data: x"), ChunkFlow::Stop);
    assert_eq!(sink.0, ["head", "chunk"]);
}

#[test]
fn a_new_ndjson_decoder_is_empty_and_clonable() {
    let decoder = NdjsonDecoder::new();
    let copy = decoder.clone();
    assert_eq!(format!("{decoder:?}"), format!("{copy:?}"));
}
