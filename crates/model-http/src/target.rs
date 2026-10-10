//! Where an endpoint is.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{AuthHeader, ExtraHeader, Timeouts};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HostName(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Port(pub u16);

/// A path on the server, beginning with `/`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UrlPath(pub String);

/// How to reach a server. Engines are on a Unix socket; TLS is for cloud hosts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
pub enum HttpTarget {
    Tcp { host: HostName, port: Port },
    Unix(PathBuf),
    Tls { host: HostName, port: Port },
}

/// Whether requests go straight to the target or through a proxy (the egress proxy of a cloud
/// backend).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Proxy {
    Direct,
    Via(Box<HttpTarget>),
}

/// Everything needed to talk to one server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpEndpoint {
    pub target: HttpTarget,
    pub proxy: Proxy,
    /// Prefixed to every request path (`/v1`).
    pub base: UrlPath,
    pub auth: AuthHeader,
    /// Sent on every request besides `auth`: Anthropic's version and beta headers, OpenRouter's
    /// referer, LiteLLM routing.
    pub headers: Vec<ExtraHeader>,
    pub timeouts: Timeouts,
}
