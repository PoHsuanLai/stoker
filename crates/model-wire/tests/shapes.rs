use model_http::{BodyKind, Exchange, Framing, HttpStatus, ResponseHead, RouteRoot, UrlPath, Verb};
use model_provider::{
    ModelInfo, ModelName, ProviderError, Tokens, TurnEnd, TurnEvent, TurnRequest,
};
use model_wire::{ChatCodec, ChatDecoder, CodecError, Driver, ErrorWire};

struct Echo;

struct EchoDecoder(Vec<String>);

impl ErrorWire for Echo {
    fn classify(&self, head: &ResponseHead, _body: &[u8]) -> ProviderError {
        match head.status {
            HttpStatus(401) => ProviderError::Unauthorized,
            _ => ProviderError::Unreadable("status".into()),
        }
    }
}

impl ChatCodec for Echo {
    type Decoder = EchoDecoder;

    fn encode(&self, _request: &TurnRequest) -> Result<Exchange, CodecError> {
        Err(CodecError::UnsupportedShape)
    }

    fn decoder(&self, _served: ModelName) -> EchoDecoder {
        EchoDecoder(vec![])
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

    fn parse_models(&self, _body: &[u8]) -> Result<Vec<ModelInfo>, CodecError> {
        Ok(vec![ModelInfo {
            name: ModelName("m".into()),
            loaded_context: Tokens(1),
            trained_context: Tokens(2),
        }])
    }
}

impl ChatDecoder for EchoDecoder {
    fn feed(&mut self, frame: &str) -> Result<Vec<TurnEvent>, CodecError> {
        self.0.push(frame.into());
        Ok(vec![TurnEvent::TextDelta(frame.into())])
    }

    fn finish(self) -> Result<TurnEnd, CodecError> {
        Err(CodecError::Truncated)
    }
}

#[test]
fn a_codec_is_implementable_without_any_io() {
    let codec = Echo;
    assert_eq!(codec.describe().path, UrlPath("/models".into()));
    assert_eq!(codec.parse_models(b"").unwrap().len(), 1);
    let mut decoder = codec.decoder(ModelName("m".into()));
    assert_eq!(
        decoder.feed("hi").unwrap(),
        vec![TurnEvent::TextDelta("hi".into())]
    );
    assert_eq!(decoder.finish(), Err(CodecError::Truncated));
}

#[test]
fn classify_reads_the_head() {
    let head = ResponseHead {
        status: HttpStatus(401),
        body: BodyKind::Json,
        retry_after: None,
        request_id: None,
    };
    assert_eq!(Echo.classify(&head, b"{}"), ProviderError::Unauthorized);
}

#[test]
fn a_driver_joins_a_codec_and_a_transport() {
    let driver = Driver::new(Echo, ());
    assert_eq!(driver.codec().describe().verb, Verb::Get);
    assert_eq!(*driver.transport(), ());
}

#[test]
fn codec_errors_are_distinct_and_print() {
    let all = [
        CodecError::UnsupportedShape,
        CodecError::Unreadable,
        CodecError::Truncated,
        CodecError::BadToolArguments,
    ];
    for (i, a) in all.iter().enumerate() {
        assert!(!a.to_string().is_empty());
        for (j, b) in all.iter().enumerate() {
            assert_eq!(i == j, a == b);
        }
    }
}
