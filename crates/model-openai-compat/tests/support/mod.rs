//! Builders for requests, and a blocking poll for futures that never wait.

#![allow(dead_code)]

use model_provider::{
    EngineExtras, ImageInput, Knob, Limits, Message, Milli, ModelName, OutputShape, Part,
    Reasoning, Role, Sampling, SchemaText, Tokens, ToolCall, ToolCallId, ToolChoice, ToolName,
    ToolParallelism, ToolResult, ToolSpec, ToolStatus, TurnRequest,
};
use serde_json::{Value, json};

pub fn block_on<T>(future: impl Future<Output = T>) -> T {
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("these transports never wait"),
    }
}

pub fn base() -> TurnRequest {
    TurnRequest {
        model: ModelName("holo".into()),
        messages: vec![],
        tools: vec![],
        tool_choice: ToolChoice::Auto,
        tool_calls: ToolParallelism::One,
        output: OutputShape::Free,
        limits: Limits {
            max_output: Tokens(256),
            stop: vec![],
        },
        sampling: Sampling {
            temperature: Milli(700),
            top_p: Knob::Off,
            top_k: Knob::Off,
            min_p: Knob::Off,
            repeat_penalty: Knob::Off,
            seed: Knob::Off,
        },
        reasoning: Reasoning::Off,
        engine: EngineExtras::None,
    }
}

pub fn msg(role: Role, parts: Vec<Part>) -> Message {
    Message { role, parts }
}

pub fn text(s: &str) -> Part {
    Part::Text(s.into())
}

pub fn user(s: &str) -> Message {
    msg(Role::User, vec![text(s)])
}

/// An image whose bytes are the ASCII of `bytes` (so its base64 is easy to write down).
pub fn image(media: &str, bytes: &[u8]) -> ImageInput {
    use base64::Engine as _;
    serde_json::from_value(json!({
        "media": media,
        "bytes": base64::engine::general_purpose::STANDARD.encode(bytes),
        "detail": "auto",
    }))
    .unwrap()
}

pub fn call(id: &str, name: &str, input: &str) -> ToolCall {
    ToolCall {
        id: ToolCallId(id.into()),
        name: ToolName::new(name).unwrap(),
        input: model_provider::JsonText::new(input).unwrap(),
    }
}

pub fn result(id: &str, parts: Vec<Part>) -> Part {
    Part::ToolResult(ToolResult {
        id: ToolCallId(id.into()),
        status: ToolStatus::Ok,
        parts,
    })
}

pub fn function(name: &str, schema: &str) -> ToolSpec {
    ToolSpec::Function {
        name: ToolName::new(name).unwrap(),
        description: format!("does {name}"),
        parameters: SchemaText(model_provider::JsonText::new(schema).unwrap()),
    }
}

pub fn body(request: &TurnRequest, flavor: model_openai_compat::Flavor) -> Value {
    let json = model_openai_compat::encode_request(request, flavor).unwrap();
    serde_json::from_str(&json.0).unwrap()
}

use model_http::{
    BodyKind, BodySink, ChunkFlow, Exchange, HttpError, HttpStatus, ResponseHead, Transport,
};
use model_replay::{
    BuildLabel, CassetteHeader, CassetteVersion, ContextStamp, EngineLabel, EngineStamp, HeadPrint,
    RecordedAt, WireBody, WireCassette, WireEnd, WireExchange, WireFrame, WireReply, WireRequest,
};

pub fn header() -> CassetteHeader {
    CassetteHeader {
        vocab: CassetteVersion::CURRENT,
        engine: EngineStamp {
            kind: EngineLabel("vllm".into()),
            build: BuildLabel("v0.12.0".into()),
        },
        model: ModelName("holo".into()),
        recorded: RecordedAt(1_790_000_000),
        context: ContextStamp {
            loaded: Tokens(8192),
            trained: Tokens(32768),
        },
        speech: None,
    }
}

pub fn chat_request(body: Option<&str>) -> WireRequest {
    WireRequest {
        verb: model_http::Verb::PostJson,
        root: model_http::RouteRoot::Base,
        path: model_http::UrlPath("/chat/completions".into()),
        body: body.map(|b| model_provider::JsonText::new(b).unwrap()),
    }
}

/// One recorded exchange whose frames are the SSE data lines.
pub fn sse_exchange(request: WireRequest, frames: &[String], end: WireEnd) -> WireExchange {
    WireExchange {
        request,
        reply: WireReply {
            head: HeadPrint {
                status: HttpStatus(200),
                body: BodyKind::EventStream,
                retry_after: None,
            },
            body: WireBody::Frames(
                frames
                    .iter()
                    .map(|data| WireFrame {
                        event: None,
                        data: data.clone(),
                    })
                    .collect(),
            ),
            end,
        },
    }
}

pub fn cassette(exchanges: Vec<WireExchange>) -> WireCassette {
    WireCassette {
        header: header(),
        exchanges,
    }
}

/// A server that answers every request with the same raw bytes: what a recording wraps.
pub struct FakeServer {
    pub head: ResponseHead,
    pub chunks: Vec<Vec<u8>>,
    pub result: Result<HttpStatus, HttpError>,
}

impl Transport for FakeServer {
    fn exchange<K: BodySink>(
        &self,
        _ex: &Exchange,
        sink: &mut K,
    ) -> impl Future<Output = Result<HttpStatus, HttpError>> + Send {
        let mut flow = sink.head(&self.head);
        for chunk in &self.chunks {
            if flow == ChunkFlow::Stop {
                break;
            }
            flow = sink.chunk(chunk);
        }
        std::future::ready(self.result)
    }
}
