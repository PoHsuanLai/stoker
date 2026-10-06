//! The TLS handshake for a `Tls` target: rustls on the ring provider, SNI from the host name,
//! ALPN `http/1.1` (the client speaks HTTP/1.1 only). The chain, the validity dates and the name
//! are always checked; there is no switch to turn that off.

use std::sync::{Arc, OnceLock};

use rustls_pki_types::{CertificateDer, ServerName};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::crypto::ring::default_provider;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};

use crate::{HostName, HttpError, TlsRoots};

/// The platform's roots, read once per process.
fn platform_store() -> &'static RootCertStore {
    static STORE: OnceLock<RootCertStore> = OnceLock::new();
    STORE.get_or_init(|| {
        let mut store = RootCertStore::empty();
        store.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
        store
    })
}

fn store_of(roots: &TlsRoots) -> RootCertStore {
    match roots {
        TlsRoots::Platform => platform_store().clone(),
        TlsRoots::Only(only) => {
            let mut store = RootCertStore::empty();
            store.add_parsable_certificates(only.iter().map(|c| CertificateDer::from(c.0.clone())));
            store
        }
    }
}

/// A connector trusting `roots`.
pub fn connector(roots: &TlsRoots) -> Result<TlsConnector, HttpError> {
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
    roots: &TlsRoots,
    name: ServerName<'static>,
    io: IO,
) -> Result<tokio_rustls::client::TlsStream<IO>, HttpError>
where
    IO: AsyncRead + AsyncWrite + Unpin,
{
    connector(roots)?
        .connect(name, io)
        .await
        .map_err(|_| HttpError::Tls)
}
