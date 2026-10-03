//! A scripted transport and a line codec for driving the `Driver` with no I/O.

#![allow(dead_code)]

use std::sync::Mutex;

use model_http::{
    BodyKind, BodySink, ChunkFlow, Exchange, Framing, HttpError, HttpStatus, JsonBody,
    ResponseHead, RouteRoot, Transport, UrlPath, Verb, WaitSeconds,
};
use model_provider::{
    Dims, EmbedEnd, EmbedTurn, EmbedVector, Flow, Knob, Limits, Milli, ModelInfo, ModelName,
    OutputShape, ProviderError, Reasoning, RetrySeconds, Sampling, StopReason, Tokens, ToolChoice,
    ToolParallelism, TurnEnd, TurnEvent, TurnRequest, TurnSink, TurnUsage,
};
use model_wire::{ChatCodec, ChatDecoder, CodecError, EmbedCodec, ErrorWire};

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

pub fn head(status: u16, body: BodyKind) -> ResponseHead {
    ResponseHead {
        status: HttpStatus(status),
        body,
        retry_after: None,
        request_id: None,
    }
}

pub fn with_retry_after(mut head: ResponseHead, seconds: u32) -> ResponseHead {
    head.retry_after = Some(WaitSeconds(seconds));
    head
}

/// One canned exchange: the head (none when the connection never opened), the chunks, the result.
#[derive(Clone)]
pub struct Canned {
    pub head: Option<ResponseHead>,
    pub chunks: Vec<Vec<u8>>,
    pub result: Result<HttpStatus, HttpError>,
}

impl Canned {
    pub fn ok(head: ResponseHead, chunks: &[&str]) -> Self {
        Canned {
            result: Ok(head.status),
            head: Some(head),
            chunks: chunks.iter().map(|c| c.as_bytes().to_vec()).collect(),
        }
    }

    pub fn rejected(head: ResponseHead, body: &str) -> Self {
        Canned {
            head: Some(head),
            chunks: vec![body.as_bytes().to_vec()],
            result: Err(HttpError::Rejected),
        }
    }

    pub fn fails(error: HttpError) -> Self {
        Canned {
            head: None,
            chunks: vec![],
            result: Err(error),
        }
    }
}

/// Plays canned exchanges in order and remembers what it was asked and what the sink said.
#[derive(Default)]
pub struct Scripted {
    pub script: Mutex<Vec<Canned>>,
    pub asked: Mutex<Vec<Exchange>>,
    pub stopped: Mutex<bool>,
}

impl Scripted {
    pub fn new(script: Vec<Canned>) -> Self {
        Scripted {
            script: Mutex::new(script),
            ..Scripted::default()
        }
    }

    pub fn asked(&self) -> Vec<Exchange> {
        self.asked.lock().unwrap().clone()
    }

    pub fn was_stopped(&self) -> bool {
        *self.stopped.lock().unwrap()
    }
}

impl Transport for Scripted {
    fn exchange<K: BodySink>(
        &self,
        ex: &Exchange,
        sink: &mut K,
    ) -> impl Future<Output = Result<HttpStatus, HttpError>> + Send {
        self.asked.lock().unwrap().push(ex.clone());
        let canned = {
            let mut script = self.script.lock().unwrap();
            if script.is_empty() {
                return std::future::ready(Err(HttpError::ReplayMiss));
            }
            script.remove(0)
        };
        let result = (|| {
            if let Some(head) = &canned.head
                && sink.head(head) == ChunkFlow::Stop
            {
                *self.stopped.lock().unwrap() = true;
                return canned.result;
            }
            for chunk in &canned.chunks {
                if sink.chunk(chunk) == ChunkFlow::Stop {
                    *self.stopped.lock().unwrap() = true;
                    return canned.result;
                }
            }
            canned.result
        })();
        std::future::ready(result)
    }
}

/// A codec whose wire is plain text: each frame is `text:<s>`, `usage:<in>,<out>`, `fault:429`,
/// `bad` (unreadable) or `done`. A reply ends only at `done`.
pub struct LineCodec {
    pub framing: Framing,
}

pub struct LineDecoder {
    served: ModelName,
    usage: TurnUsage,
    done: bool,
    fault: Option<ProviderError>,
    pub seen: Vec<String>,
}

impl ErrorWire for LineCodec {
    fn classify(&self, head: &ResponseHead, body: &[u8]) -> ProviderError {
        match head.status.0 {
            401 => ProviderError::Unauthorized,
            429 => ProviderError::RateLimited(RetrySeconds(head.retry_after.map_or(0, |w| w.0))),
            200 => ProviderError::Unreadable(format!("html {} bytes", body.len())),
            n => ProviderError::BadRequest(format!("{n} {} bytes", body.len())),
        }
    }
}

impl ChatCodec for LineCodec {
    type Decoder = LineDecoder;

    fn encode(&self, request: &TurnRequest) -> Result<Exchange, CodecError> {
        if request.model.0 == "unsupported" {
            return Err(CodecError::UnsupportedShape);
        }
        Ok(Exchange {
            verb: Verb::PostJson,
            root: RouteRoot::Base,
            path: UrlPath("/chat".into()),
            body: Some(JsonBody(format!("{{\"model\":\"{}\"}}", request.model.0))),
            framing: self.framing,
        })
    }

    fn decoder(&self, served: ModelName) -> LineDecoder {
        LineDecoder {
            served,
            usage: TurnUsage::default(),
            done: false,
            fault: None,
            seen: vec![],
        }
    }

    fn describe(&self) -> Exchange {
        Exchange {
            verb: Verb::Get,
            root: RouteRoot::Base,
            path: UrlPath("/models".into()),
            body: None,
            framing: Framing::Whole,
        }
    }

    fn parse_models(&self, body: &[u8]) -> Result<Vec<ModelInfo>, CodecError> {
        let text = std::str::from_utf8(body).map_err(|_| CodecError::Unreadable)?;
        text.lines()
            .map(|line| {
                let (name, ctx) = line.split_once(' ').ok_or(CodecError::Unreadable)?;
                let ctx: u32 = ctx.parse().map_err(|_| CodecError::Unreadable)?;
                Ok(ModelInfo {
                    name: ModelName(name.to_owned()),
                    loaded_context: Tokens(ctx),
                    trained_context: Tokens(ctx),
                })
            })
            .collect()
    }
}

impl ChatDecoder for LineDecoder {
    fn feed(&mut self, frame: &str) -> Result<Vec<TurnEvent>, CodecError> {
        self.seen.push(frame.to_owned());
        match frame.split_once(':') {
            Some(("text", t)) => Ok(vec![TurnEvent::TextDelta(t.to_owned())]),
            Some(("usage", u)) => {
                let (i, o) = u.split_once(',').ok_or(CodecError::Unreadable)?;
                self.usage = TurnUsage {
                    input: Tokens(i.parse().map_err(|_| CodecError::Unreadable)?),
                    output: Tokens(o.parse().map_err(|_| CodecError::Unreadable)?),
                    ..TurnUsage::default()
                };
                Ok(vec![TurnEvent::Usage(self.usage)])
            }
            Some(("fault", _)) => {
                self.fault = Some(ProviderError::RateLimited(RetrySeconds(0)));
                Err(CodecError::Unreadable)
            }
            _ if frame == "done" => {
                self.done = true;
                Ok(vec![])
            }
            _ => Err(CodecError::Unreadable),
        }
    }

    fn fault(&self) -> Option<ProviderError> {
        self.fault.clone()
    }

    fn finish(self) -> Result<TurnEnd, CodecError> {
        if self.done {
            Ok(TurnEnd {
                stop: StopReason::EndTurn,
                usage: self.usage,
                served: self.served,
            })
        } else {
            Err(CodecError::Truncated)
        }
    }
}

/// An embedding codec over the same plain text: `a,b;c,d` is two vectors.
pub struct VecCodec;

impl ErrorWire for VecCodec {
    fn classify(&self, head: &ResponseHead, _body: &[u8]) -> ProviderError {
        ProviderError::BadRequest(format!("{}", head.status.0))
    }
}

impl EmbedCodec for VecCodec {
    fn encode_embed(&self, turn: &EmbedTurn) -> Result<Exchange, CodecError> {
        Ok(Exchange {
            verb: Verb::PostJson,
            root: RouteRoot::Base,
            path: UrlPath("/embeddings".into()),
            body: Some(JsonBody(format!("{}", turn.inputs.len()))),
            framing: Framing::Whole,
        })
    }

    fn decode_embed(&self, served: ModelName, body: &[u8]) -> Result<EmbedEnd, CodecError> {
        let text = std::str::from_utf8(body).map_err(|_| CodecError::Unreadable)?;
        let vectors = text
            .split(';')
            .filter(|v| !v.is_empty())
            .map(|v| {
                v.split(',')
                    .map(|n| n.parse::<f32>().map_err(|_| CodecError::Unreadable))
                    .collect::<Result<Vec<f32>, _>>()
                    .map(EmbedVector)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(EmbedEnd {
            vectors,
            usage: TurnUsage::default(),
            served,
        })
    }
}

pub fn request(model: &str) -> TurnRequest {
    TurnRequest {
        model: ModelName(model.into()),
        messages: vec![],
        tools: vec![],
        tool_choice: ToolChoice::Auto,
        tool_calls: ToolParallelism::One,
        output: OutputShape::Free,
        limits: Limits {
            max_output: Tokens(16),
            stop: vec![],
        },
        sampling: Sampling {
            temperature: Milli(0),
            top_p: Knob::Off,
            top_k: Knob::Off,
            min_p: Knob::Off,
            repeat_penalty: Knob::Off,
            seed: Knob::Off,
        },
        reasoning: Reasoning::Off,
        engine: model_provider::EngineExtras::None,
    }
}

pub fn embed_turn(inputs: &[&str], dims: Knob<Dims>) -> EmbedTurn {
    EmbedTurn {
        model: ModelName("e".into()),
        inputs: inputs.iter().map(|s| (*s).to_owned()).collect(),
        role: model_provider::EmbedRole::Document,
        dims,
    }
}

/// Keeps events; stops after `stop_after` of them when set.
#[derive(Default)]
pub struct Keep {
    pub events: Vec<TurnEvent>,
    pub stop_after: Option<usize>,
}

impl TurnSink for Keep {
    fn event(&mut self, event: TurnEvent) -> Flow {
        self.events.push(event);
        match self.stop_after {
            Some(n) if self.events.len() >= n => Flow::Stop,
            _ => Flow::Continue,
        }
    }
}
