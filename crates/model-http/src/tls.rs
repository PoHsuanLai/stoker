//! The TLS handshake for a `Tls` target: rustls on the ring provider, SNI from the host name,
//! ALPN `http/1.1` (the client speaks HTTP/1.1 only). The chain, the validity dates and the name
//! are always checked; there is no switch to turn that off.
//!
//! The connector is built from a client's roots once, by [`Cache`], which the client owns. No
//! state is shared between clients: two clients with different roots never see each other's.

use std::sync::{Arc, OnceLock};

use rustls_pki_types::{CertificateDer, ServerName};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::crypto::ring::default_provider;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};

use crate::{HostName, HttpError, TlsRoots};

/// One client's connector, built on the first `Tls` exchange and reused by every later one.
#[derive(Default)]
pub struct Cache(OnceLock<Result<TlsConnector, HttpError>>);

impl Cache {
    /// The connector for `roots`. The roots are the client's own and never change, so the first
    /// call decides what the cache holds.
    pub fn get(&self, roots: &TlsRoots) -> Result<&TlsConnector, HttpError> {
        self.0
            .get_or_init(|| connector(roots))
            .as_ref()
            .map_err(|e| *e)
    }
}

impl core::fmt::Debug for Cache {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TlsCache")
    }
}

fn store_of(roots: &TlsRoots) -> RootCertStore {
    let mut store = RootCertStore::empty();
    match roots {
        TlsRoots::Platform => {
            store.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
        }
        TlsRoots::Only(only) => {
            store.add_parsable_certificates(only.iter().map(|c| CertificateDer::from(c.0.clone())));
        }
    }
    store
}

/// A connector trusting `roots`.
fn connector(roots: &TlsRoots) -> Result<TlsConnector, HttpError> {
    let mut config = ClientConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|_| HttpError::Tls)?
        .with_root_certificates(store_of(roots))
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(TlsConnector::from(Arc::new(config)))
}

/// The name the certificate must match, and the SNI sent; a host that is not a DNS name or an IP
/// address is `HttpError::Tls`, found before anything is connected.
pub fn server_name(host: &HostName) -> Result<ServerName<'static>, HttpError> {
    ServerName::try_from(host.0.clone()).map_err(|_| HttpError::Tls)
}

/// The handshake over `io`; any failure (an untrusted or expired chain, a name mismatch, a server
/// that is not TLS) is `HttpError::Tls`.
pub async fn handshake<IO>(
    connector: &TlsConnector,
    name: ServerName<'static>,
    io: IO,
) -> Result<tokio_rustls::client::TlsStream<IO>, HttpError>
where
    IO: AsyncRead + AsyncWrite + Unpin,
{
    connector
        .connect(name, io)
        .await
        .map_err(|_| HttpError::Tls)
}
