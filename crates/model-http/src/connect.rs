//! The reqwest client behind `Transport for HttpClient`: one per `HttpClient`, built on its first
//! exchange and shared by its clones.
//!
//! It is built to give the old per-exchange behaviour: no redirect is followed, no proxy from the
//! environment is used, no connection is kept after its exchange (so dropping the exchange drops
//! the connection, which aborts generation in the engine), and the connect timeout covers the TCP
//! connect and the TLS handshake. A `Unix` target is reqwest's `unix_socket`; a `Tls` target is
//! rustls on the ring provider. The chain, the validity dates and the name are always checked;
//! there is no switch to turn that off.

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::{Client, ClientBuilder, redirect};

use crate::{HttpEndpoint, HttpError, HttpTarget, TlsRoots, WaitMs};

/// One client's reqwest client, or the reason it could not be built.
#[derive(Default)]
pub struct Wire(OnceLock<Result<Client, HttpError>>);

impl Wire {
    /// The client for `endpoint` and `roots`. Both are the owner's and never change, so the first
    /// call decides what the cell holds.
    pub fn get(&self, endpoint: &HttpEndpoint, roots: &TlsRoots) -> Result<&Client, HttpError> {
        self.0
            .get_or_init(|| build(endpoint, roots))
            .as_ref()
            .map_err(|e| *e)
    }
}

impl core::fmt::Debug for Wire {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Wire")
    }
}

/// `wait` as a duration.
pub fn millis(wait: WaitMs) -> Duration {
    Duration::from_millis(u64::from(wait.0))
}

fn build(endpoint: &HttpEndpoint, roots: &TlsRoots) -> Result<Client, HttpError> {
    let base = Client::builder()
        .redirect(redirect::Policy::none())
        .no_proxy()
        .pool_max_idle_per_host(0)
        .connect_timeout(millis(endpoint.timeouts.connect));
    let builder = match &endpoint.target {
        HttpTarget::Unix(path) => base.unix_socket(path.as_path()),
        _ => base,
    };
    secure(builder, &endpoint.target, roots)?
        .build()
        .map_err(|_| HttpError::Connect)
}

#[cfg(feature = "tls")]
mod rustls_ring {
    use std::sync::Once;

    /// Installs ring as the process's rustls provider, once. reqwest is built without a provider
    /// (stoker uses ring, never aws-lc) and reads the process default when it builds a client. A
    /// provider somebody else installed first stays.
    pub fn install() {
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
    }
}

/// The builder with TLS settled: the platform's roots, or only the given ones.
#[cfg(feature = "tls")]
fn secure(
    builder: ClientBuilder,
    _target: &HttpTarget,
    roots: &TlsRoots,
) -> Result<ClientBuilder, HttpError> {
    rustls_ring::install();
    match roots {
        TlsRoots::Platform => Ok(builder),
        TlsRoots::Only(only) => {
            let certs = only
                .iter()
                .map(|cert| reqwest::Certificate::from_der(&cert.0))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| HttpError::Tls)?;
            Ok(builder.tls_certs_only(certs))
        }
    }
}

/// Without the `tls` feature a `Tls` target answers `HttpError::Tls` before anything is sent.
#[cfg(not(feature = "tls"))]
fn secure(
    builder: ClientBuilder,
    target: &HttpTarget,
    _roots: &TlsRoots,
) -> Result<ClientBuilder, HttpError> {
    match target {
        HttpTarget::Tls { .. } => Err(HttpError::Tls),
        _ => Ok(builder),
    }
}

/// Whether `error` or anything under it is a TLS failure (an untrusted or expired chain, a name
/// mismatch, a peer that is not TLS, a peer that hung up in the handshake) and not a failure to
/// reach the host.
#[cfg(feature = "tls")]
pub fn is_tls_failure(error: &(dyn std::error::Error + 'static)) -> bool {
    use std::error::Error;
    use std::io::ErrorKind;
    let mut next = Some(error);
    while let Some(err) = next {
        if err.is::<rustls::Error>() {
            return true;
        }
        if let Some(io) = err.downcast_ref::<std::io::Error>() {
            // `io::Error::source` skips the error it wraps, so look inside it here.
            let wrapped = io
                .get_ref()
                .is_some_and(|inner| inner.is::<rustls::Error>());
            if wrapped || io.kind() == ErrorKind::UnexpectedEof {
                return true;
            }
        }
        next = err.source();
    }
    false
}

#[cfg(not(feature = "tls"))]
pub fn is_tls_failure(_error: &(dyn std::error::Error + 'static)) -> bool {
    false
}
