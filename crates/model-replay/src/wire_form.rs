//! The pure half of wire cassettes: the scrubbed request a codec built, frames to bytes and back,
//! and the plans that slice bytes into chunks.

use model_http::{Exchange, Framing, HttpStatus, ResponseHead};
use model_provider::JsonText;
use serde_json::Value;

use crate::canon::{canonical, digest_hex, len64};
use crate::{ByteStep, ChunkPlan, HeadPrint, WireBody, WireFrame, WireRequest};

/// What replaces an absolute model path (llama.cpp echoes the full GGUF path in `model`).
pub(crate) const REDACTED_PATH: &str = "/REDACTED_PATH";

/// Keys whose absolute-path values are scrubbed.
const PATH_KEYS: [&str; 2] = ["model", "model_path"];

/// The scrubbed request of an exchange: canonical JSON, image data replaced by an image print,
/// absolute model paths redacted. Recording and matching both go through it.
pub(crate) fn request_of(ex: &Exchange) -> WireRequest {
    WireRequest {
        verb: ex.verb,
        root: ex.root,
        path: ex.path.clone(),
        body: ex.body.as_ref().and_then(|b| {
            let value = serde_json::from_str(&b.0).unwrap_or_else(|_| Value::String(b.0.clone()));
            JsonText::new(canonical(&scrub(value))).ok()
        }),
    }
}

/// Two requests are the same when the routes are equal and the bodies are the same JSON.
pub(crate) fn same_request(a: &WireRequest, b: &WireRequest) -> bool {
    let body = |r: &WireRequest| {
        r.body
            .as_ref()
            .and_then(|b| serde_json::from_str::<Value>(b.as_str()).ok())
            .map(|v| canonical(&v))
    };
    (a.verb, a.root, &a.path) == (b.verb, b.root, &b.path) && body(a) == body(b)
}

fn scrub(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, v)| match v {
                    Value::String(s) if PATH_KEYS.contains(&key.as_str()) && s.starts_with('/') => {
                        (key, Value::String(REDACTED_PATH.to_owned()))
                    }
                    other => (key, scrub(other)),
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(scrub).collect()),
        Value::String(s) => Value::String(image_print(&s).unwrap_or(s)),
        other => other,
    }
}

/// `data:image/png;base64,<payload>` becomes `data:image/png;print=blake3:<hex>;len=<bytes>`.
fn image_print(text: &str) -> Option<String> {
    let rest = text.strip_prefix("data:image/")?;
    let (media, payload) = rest.split_once(";base64,")?;
    Some(format!(
        "data:image/{media};print=blake3:{};len={}",
        digest_hex(payload.as_bytes()),
        len64(payload.len())
    ))
}

/// Scrubs one frame's or body's text: JSON is parsed and only rewritten when something changed.
pub(crate) fn scrub_text(text: &str) -> String {
    match serde_json::from_str::<Value>(text) {
        Ok(value) => {
            let scrubbed = scrub(value.clone());
            if scrubbed == value {
                text.to_owned()
            } else {
                canonical(&scrubbed)
            }
        }
        Err(_) => text.to_owned(),
    }
}

pub(crate) fn head_print(head: &ResponseHead) -> HeadPrint {
    HeadPrint {
        status: head.status,
        body: head.body,
        retry_after: head.retry_after,
    }
}

/// Success as an HTTP status: a transport answers anything else with `Rejected`.
pub(crate) fn is_success(status: HttpStatus) -> bool {
    (200..300).contains(&status.0)
}

/// One frame as the bytes a server would send it in.
pub(crate) fn frame_bytes(frame: &WireFrame, framing: Framing) -> Vec<u8> {
    let text = match framing {
        Framing::Sse => {
            let event = frame
                .event
                .as_ref()
                .map(|e| format!("event: {}\n", e.0))
                .unwrap_or_default();
            let data: String = frame
                .data
                .split('\n')
                .map(|l| format!("data: {l}\n"))
                .collect();
            format!("{event}{data}\n")
        }
        Framing::Ndjson | Framing::Whole => format!("{}\n", frame.data),
    };
    text.into_bytes()
}

/// The body as the chunks a plan cuts it into.
pub(crate) fn chunks(body: &WireBody, framing: Framing, plan: ChunkPlan) -> Vec<Vec<u8>> {
    let per_frame: Vec<Vec<u8>> = match body {
        WireBody::Whole(text) => vec![text.clone().into_bytes()],
        WireBody::Frames(frames) => frames.iter().map(|f| frame_bytes(f, framing)).collect(),
    };
    match plan {
        ChunkPlan::Lines => per_frame.into_iter().filter(|b| !b.is_empty()).collect(),
        ChunkPlan::Whole => {
            let all: Vec<u8> = per_frame.concat();
            if all.is_empty() {
                Vec::new()
            } else {
                vec![all]
            }
        }
        ChunkPlan::Every(ByteStep(n)) => {
            let step = usize::try_from(n.max(1)).unwrap_or(1);
            per_frame
                .concat()
                .chunks(step)
                .map(<[u8]>::to_vec)
                .collect()
        }
        ChunkPlan::Seeded(seed) => {
            let bytes = per_frame.concat();
            let mut state = u64::from(seed.0);
            let mut rest = bytes.as_slice();
            let mut out = Vec::new();
            while !rest.is_empty() {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let size = usize::try_from(1 + (state >> 33) % 16)
                    .unwrap_or(1)
                    .min(rest.len());
                let (head, tail) = rest.split_at(size);
                out.push(head.to_vec());
                rest = tail;
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model_http::{EventName, JsonBody, RouteRoot, UrlPath, Verb};
    use model_provider::Seed;

    fn post(body: &str) -> Exchange {
        Exchange {
            verb: Verb::PostJson,
            root: RouteRoot::Base,
            path: UrlPath("/chat/completions".into()),
            body: Some(JsonBody(body.into())),
            framing: Framing::Sse,
        }
    }

    fn body_of(ex: &Exchange) -> String {
        request_of(ex).body.expect("a body").as_str().to_owned()
    }

    #[test]
    fn requests_are_canonical_and_scrubbed() {
        let table = [
            (
                r#"{"stream":true,"model":"holo"}"#,
                r#"{"model":"holo","stream":true}"#,
            ),
            (
                r#"{"model":"/home/me/models/holo.gguf"}"#,
                r#"{"model":"/REDACTED_PATH"}"#,
            ),
            (
                r#"{"input":"/not/a/model path"}"#,
                r#"{"input":"/not/a/model path"}"#,
            ),
            (
                r#"[{"model_path":"/x"}]"#,
                r#"[{"model_path":"/REDACTED_PATH"}]"#,
            ),
        ];
        for (raw, want) in table {
            assert_eq!(body_of(&post(raw)), want, "{raw}");
        }
    }

    #[test]
    fn images_become_prints() {
        let got = body_of(&post(r#"{"url":"data:image/png;base64,AAAA"}"#));
        let want = format!(
            r#"{{"url":"data:image/png;print=blake3:{};len=4"}}"#,
            digest_hex(b"AAAA")
        );
        assert_eq!(got, want);
        assert!(!got.contains("AAAA"));
    }

    #[test]
    fn same_request_ignores_key_order() {
        let a = request_of(&post(r#"{"a":1,"b":2}"#));
        let b = WireRequest {
            body: Some(JsonText::new(r#"{ "b": 2, "a": 1 }"#).unwrap()),
            ..a.clone()
        };
        assert!(same_request(&a, &b));
        assert!(!same_request(&a, &request_of(&post(r#"{"a":1}"#))));
    }

    #[test]
    fn scrub_text_keeps_untouched_text_verbatim() {
        assert_eq!(scrub_text("[DONE]"), "[DONE]");
        assert_eq!(
            scrub_text(r#"{ "b": 1,  "a": 2 }"#),
            r#"{ "b": 1,  "a": 2 }"#
        );
        assert_eq!(
            scrub_text(r#"{"model":"/opt/m.gguf","a":1}"#),
            r#"{"a":1,"model":"/REDACTED_PATH"}"#
        );
    }

    fn frames() -> WireBody {
        WireBody::Frames(vec![
            WireFrame {
                event: None,
                data: "one".into(),
            },
            WireFrame {
                event: Some(EventName("ping".into())),
                data: "a\nb".into(),
            },
        ])
    }

    #[test]
    fn frames_serialise_per_framing() {
        let f = frames();
        let join =
            |framing| String::from_utf8(chunks(&f, framing, ChunkPlan::Whole).concat()).unwrap();
        assert_eq!(
            join(Framing::Sse),
            "data: one\n\nevent: ping\ndata: a\ndata: b\n\n"
        );
        assert_eq!(join(Framing::Ndjson), "one\na\nb\n");
    }

    #[test]
    fn every_plan_cuts_the_same_bytes() {
        let f = frames();
        let whole = chunks(&f, Framing::Sse, ChunkPlan::Whole).concat();
        let plans = [
            ChunkPlan::Lines,
            ChunkPlan::Every(ByteStep(1)),
            ChunkPlan::Every(ByteStep(0)),
            ChunkPlan::Every(ByteStep(7)),
            ChunkPlan::Seeded(Seed(1)),
            ChunkPlan::Seeded(Seed(99)),
        ];
        for plan in plans {
            assert_eq!(chunks(&f, Framing::Sse, plan).concat(), whole, "{plan:?}");
        }
        assert_eq!(chunks(&f, Framing::Sse, ChunkPlan::Lines).len(), 2);
        assert_eq!(
            chunks(&f, Framing::Sse, ChunkPlan::Every(ByteStep(1))).len(),
            whole.len()
        );
        let seeded = chunks(&f, Framing::Sse, ChunkPlan::Seeded(Seed(1)));
        assert!(seeded.iter().all(|c| (1..=16).contains(&c.len())));
        assert_eq!(
            seeded,
            chunks(&f, Framing::Sse, ChunkPlan::Seeded(Seed(1))),
            "seeded is deterministic"
        );
    }

    #[test]
    fn an_empty_body_has_no_chunks() {
        let empty = WireBody::Whole(String::new());
        assert!(chunks(&empty, Framing::Whole, ChunkPlan::Whole).is_empty());
        assert!(chunks(&WireBody::Frames(vec![]), Framing::Sse, ChunkPlan::Lines).is_empty());
    }
}
