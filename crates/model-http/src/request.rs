//! The request a transport sends: the URL, the headers and the body of one exchange or upload
//! against an endpoint. Nothing is connected here; a request that cannot be written is never sent.

use reqwest::header::{
    ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName as WireName, HeaderValue,
};
use reqwest::{Method, Url};

use crate::{
    AuthHeader, Exchange, Framing, HttpEndpoint, HttpError, HttpTarget, RouteRoot, Upload, Verb,
};

/// What a request is, whichever seam asked for it.
pub struct Parts<'a> {
    pub method: Method,
    pub root: RouteRoot,
    pub path: &'a str,
    pub framing: Framing,
    pub content_type: Option<&'a str>,
    pub body: Vec<u8>,
}

impl<'a> Parts<'a> {
    pub fn of_exchange(ex: &'a Exchange) -> Self {
        Self {
            method: match ex.verb {
                Verb::Get => Method::GET,
                Verb::PostJson => Method::POST,
            },
            root: ex.root,
            path: ex.path.0.as_str(),
            framing: ex.framing,
            content_type: ex.body.as_ref().map(|_| "application/json"),
            body: ex
                .body
                .as_ref()
                .map_or_else(Vec::new, |b| b.0.clone().into_bytes()),
        }
    }

    pub fn of_upload(up: &'a Upload) -> Self {
        Self {
            method: Method::POST,
            root: up.root,
            path: up.path.0.as_str(),
            framing: up.framing,
            content_type: Some(up.body.content_type.0.as_str()),
            body: up.body.bytes.clone(),
        }
    }
}

/// A request ready to hand to the client.
pub struct Prepared {
    pub method: Method,
    pub url: Url,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

/// Whether a key may go to a plain `Tcp` host on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Loopback {
    Refused,
    Allowed,
}

/// A key goes over TLS or a Unix socket; over plain `Tcp` only to a loopback host, and only when
/// the caller opted in.
fn allow_key(target: &HttpTarget, loopback: Loopback) -> Result<(), HttpError> {
    match target {
        HttpTarget::Tcp { host, .. } if loopback == Loopback::Allowed && is_loopback(&host.0) => {
            Ok(())
        }
        HttpTarget::Tcp { .. } => Err(HttpError::Insecure),
        _ => Ok(()),
    }
}

fn is_loopback(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    bare.eq_ignore_ascii_case("localhost")
        || bare
            .parse::<core::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// The URL of `path` on `target`. A Unix socket has no host; reqwest ignores the name, and
/// `localhost` is what the `Host` header says. A host that is not a name or an address is
/// `HttpError::Tls` for a `Tls` target (found before anything is connected) and `Connect` for
/// the others.
fn url_of(target: &HttpTarget, path: &str) -> Result<Url, HttpError> {
    let (scheme, authority, wrong) = match target {
        HttpTarget::Tcp { host, port } => ("http", authority(&host.0, port.0), HttpError::Connect),
        HttpTarget::Tls { host, port } => ("https", authority(&host.0, port.0), HttpError::Tls),
        HttpTarget::Unix(_) => ("http", "localhost".to_owned(), HttpError::Connect),
    };
    Url::parse(&format!("{scheme}://{authority}{path}")).map_err(|_| wrong)
}

/// `host:port`, with a bare IPv6 address in brackets.
fn authority(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// The URL, headers and body of `parts` against `endpoint`. A header value that cannot be
/// written (a newline in a secret) is a request that is never sent.
pub fn prepare(endpoint: &HttpEndpoint, parts: Parts<'_>) -> Result<Prepared, HttpError> {
    let path = match parts.root {
        RouteRoot::Base => format!("{}{}", endpoint.base.0, parts.path),
        RouteRoot::Server => parts.path.to_owned(),
    };
    let accept = match parts.framing {
        Framing::Sse => "text/event-stream",
        Framing::Ndjson => "application/x-ndjson",
        Framing::Whole => "application/json",
    };
    let mut headers = HeaderMap::new();
    headers.append(ACCEPT, HeaderValue::from_static(accept));
    if let Some(content_type) = parts.content_type {
        let value = HeaderValue::from_str(content_type).map_err(|_| HttpError::Connect)?;
        headers.append(CONTENT_TYPE, value);
    }
    let secret = |name: WireName, value: &str| {
        HeaderValue::from_str(value)
            .map(|mut value| {
                value.set_sensitive(true);
                (name, value)
            })
            .map_err(|_| HttpError::Connect)
    };
    let mut keyed = Vec::new();
    match &endpoint.auth {
        AuthHeader::None => {}
        AuthHeader::Bearer(token) => {
            allow_key(&endpoint.target, Loopback::Refused)?;
            keyed.push(secret(AUTHORIZATION, &format!("Bearer {}", token.0))?);
        }
        AuthHeader::LoopbackBearer(token) => {
            allow_key(&endpoint.target, Loopback::Allowed)?;
            keyed.push(secret(AUTHORIZATION, &format!("Bearer {}", token.0))?);
        }
        AuthHeader::Header { name, value } => {
            let name = WireName::from_bytes(name.0.as_bytes()).map_err(|_| HttpError::Connect)?;
            keyed.push(secret(name, &value.0)?);
        }
    }
    for extra in &endpoint.headers {
        let name = WireName::from_bytes(extra.name.0.as_bytes()).map_err(|_| HttpError::Connect)?;
        keyed.push(secret(name, &extra.value.0)?);
    }
    for (name, value) in keyed {
        headers.append(name, value);
    }
    Ok(Prepared {
        method: parts.method,
        url: url_of(&endpoint.target, &path)?,
        headers,
        body: parts.body,
    })
}
