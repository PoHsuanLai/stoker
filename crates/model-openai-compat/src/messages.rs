//! Messages as the chat-completions wire writes them.
//!
//! A tool result is a `role: "tool"` message whatever message of ours carried it, so a message
//! with results is split around them. Images in a tool result stay inside the tool message where
//! the server honours that (llama-server), and otherwise follow in a user message once the
//! consecutive tool messages are over (a user message between two tool messages breaks the
//! pairing with the assistant's calls). Thoughts are not handed back: these servers' templates
//! drop the reasoning of earlier turns.

use model_provider::{ImageInput, Message, Part, Role, ToolCall, ToolResult};
use model_wire::CodecError;
use serde_json::{Value, json};

use crate::ToolImages;

/// Every message of a request, in wire order.
pub(crate) fn encode(messages: &[Message], images: ToolImages) -> Result<Vec<Value>, CodecError> {
    let mut out = Vec::new();
    let mut after_tools = Vec::new();
    for message in messages {
        let mut run: Vec<&Part> = Vec::new();
        for part in &message.parts {
            match part {
                Part::ToolResult(result) => {
                    emit_run(&mut out, &mut after_tools, message.role, &mut run)?;
                    tool_result(&mut out, &mut after_tools, result, images)?;
                }
                other => run.push(other),
            }
        }
        emit_run(&mut out, &mut after_tools, message.role, &mut run)?;
    }
    flush_images(&mut out, &mut after_tools);
    Ok(out)
}

/// The images held back from tool messages, as one user message.
fn flush_images(out: &mut Vec<Value>, held: &mut Vec<Value>) {
    if !held.is_empty() {
        out.push(json!({ "role": "user", "content": std::mem::take(held) }));
    }
}

/// A message that is not a tool message ends the run of tool messages: the images held back from
/// them go first.
fn emit_run(
    out: &mut Vec<Value>,
    after_tools: &mut Vec<Value>,
    role: Role,
    run: &mut Vec<&Part>,
) -> Result<(), CodecError> {
    if run.is_empty() {
        return Ok(());
    }
    flush_images(out, after_tools);
    let parts = std::mem::take(run);
    out.push(match role {
        Role::System => system(&parts)?,
        Role::Assistant => assistant(&parts)?,
        Role::User | Role::Tool => user(&parts)?,
    });
    Ok(())
}

fn system(parts: &[&Part]) -> Result<Value, CodecError> {
    let texts = parts
        .iter()
        .map(|part| match part {
            Part::Text(text) => Ok(text.as_str()),
            _ => Err(CodecError::UnsupportedShape),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "role": "system", "content": texts.join("\n\n") }))
}

fn user(parts: &[&Part]) -> Result<Value, CodecError> {
    let content = match parts {
        [Part::Text(text)] => Value::String(text.clone()),
        _ => Value::Array(
            parts
                .iter()
                .map(|p| content_part(p))
                .collect::<Result<_, _>>()?,
        ),
    };
    Ok(json!({ "role": "user", "content": content }))
}

fn assistant(parts: &[&Part]) -> Result<Value, CodecError> {
    let mut text = String::new();
    let mut calls = Vec::new();
    for part in parts {
        match part {
            Part::Text(t) => text.push_str(t),
            Part::ToolCall(call) => calls.push(tool_call(call)),
            Part::Thought { .. } => {}
            // Image, ToolResult and any part kind this build does not know cannot be sent.
            _ => return Err(CodecError::UnsupportedShape),
        }
    }
    let mut message = json!({ "role": "assistant", "content": text });
    if !calls.is_empty() {
        message["tool_calls"] = Value::Array(calls);
    }
    Ok(message)
}

fn tool_call(call: &ToolCall) -> Value {
    json!({
        "id": call.id.0,
        "type": "function",
        "function": { "name": call.name.as_str(), "arguments": call.input.as_str() },
    })
}

fn tool_result(
    out: &mut Vec<Value>,
    after_tools: &mut Vec<Value>,
    result: &ToolResult,
    images: ToolImages,
) -> Result<(), CodecError> {
    let mut texts = Vec::new();
    let mut pictures = Vec::new();
    for part in &result.parts {
        match part {
            Part::Text(text) => texts.push(text.as_str()),
            Part::Image(image) => pictures.push(image_part(image)),
            Part::Thought { .. } => {}
            _ => return Err(CodecError::UnsupportedShape),
        }
    }
    let text = texts.join("\n");
    let content = match (images, pictures.is_empty()) {
        (_, true) => Value::String(text),
        (ToolImages::InToolMessage, false) => {
            let mut parts = vec![json!({ "type": "text", "text": text })];
            parts.append(&mut pictures);
            Value::Array(parts)
        }
        (ToolImages::NextUserMessage, false) => {
            after_tools.append(&mut pictures);
            Value::String(text)
        }
    };
    out.push(json!({ "role": "tool", "tool_call_id": result.id.0, "content": content }));
    Ok(())
}

fn content_part(part: &Part) -> Result<Value, CodecError> {
    match part {
        Part::Text(text) => Ok(json!({ "type": "text", "text": text })),
        Part::Image(image) => Ok(image_part(image)),
        // Thought, ToolCall, ToolResult and any part kind this build does not know.
        _ => Err(CodecError::UnsupportedShape),
    }
}

/// `data:<media>;base64,<bytes>`: the media and the base64 come from the types' own serde forms.
fn image_part(image: &ImageInput) -> Value {
    let media = serde_json::to_value(image.media)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let bytes = serde_json::to_value(&image.bytes)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let mut url = json!({ "url": format!("data:image/{media};base64,{bytes}") });
    if image.detail == model_provider::ImageDetail::Original {
        url["detail"] = Value::String("high".into());
    }
    json!({ "type": "image_url", "image_url": url })
}
