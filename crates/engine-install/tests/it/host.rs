//! A fake file host for downloads: `https` on loopback with a scratch CA (so a client must trust
//! it, as it would any host), files served by path with `Range` support, redirects, and a record
//! of every request. A test puts the bytes it wants served and knows their hash. Nothing leaves
//! the machine.

use model_http::{DerCertificate, TlsRoots};
use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair};
use rustls_pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::ring::default_provider;

/// One request the host answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The request target.
    pub target: String,
    /// The `Range` header, when there was one.
    pub range: Option<String>,
    /// The status it answered.
    pub status: u16,
}

#[derive(Debug, Default)]
struct Shared {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
    redirects: Mutex<BTreeMap<String, String>>,
    hits: Mutex<Vec<Hit>>,
}

fn held<T>(cell: &Mutex<T>) -> MutexGuard<'_, T> {
    cell.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A file host serving on a task; dropping it stops the server.
#[derive(Debug)]
pub struct FileHost {
    port: u16,
    ca: DerCertificate,
    shared: Arc<Shared>,
    task: JoinHandle<()>,
}

impl Drop for FileHost {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl FileHost {
    /// Binds a host on loopback and serves it on a task.
    pub async fn start() -> Self {
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).expect("ca params");
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca_key = KeyPair::generate().expect("ca key");
        let ca = ca_params.self_signed(&ca_key).expect("ca");
        let leaf_key = KeyPair::generate().expect("leaf key");
        let leaf = CertificateParams::new(vec!["localhost".to_owned()])
            .expect("leaf params")
            .signed_by(&leaf_key, &ca, &ca_key)
            .expect("leaf");
        let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));
        let mut config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
            .with_safe_default_protocol_versions()
            .expect("protocol versions")
            .with_no_client_auth()
            .with_single_cert(vec![leaf.der().clone()], key)
            .expect("server certificate");
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        let shared = Arc::new(Shared::default());
        let serving = Arc::clone(&shared);
        let task = tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let (acceptor, shared) = (acceptor.clone(), Arc::clone(&serving));
                tokio::spawn(async move {
                    if let Ok(tls) = acceptor.accept(tcp).await {
                        serve(tls, &shared).await;
                    }
                });
            }
        });
        Self {
            port,
            ca: DerCertificate(ca.der().to_vec()),
            shared,
            task,
        }
    }

    /// The roots a client must be given to trust this host.
    pub fn roots(&self) -> TlsRoots {
        TlsRoots::Only(vec![self.ca.clone()])
    }

    /// The address of `path` (which starts with `/`) on this host.
    pub fn url(&self, path: &str) -> String {
        format!("https://localhost:{}{path}", self.port)
    }

    /// Serves `body` at `path`.
    pub fn put(&self, path: &str, body: impl Into<Vec<u8>>) {
        held(&self.shared.files).insert(path.to_owned(), body.into());
    }

    /// Answers `from` with a redirect to `to`, both paths on this host.
    pub fn redirect(&self, from: &str, to: &str) {
        let target = self.url(to);
        held(&self.shared.redirects).insert(from.to_owned(), target);
    }

    /// Every request answered, oldest first.
    pub fn hits(&self) -> Vec<Hit> {
        held(&self.shared.hits).clone()
    }
}

/// Reads one request from `tls`, answers it and closes.
async fn serve<S: AsyncReadExt + AsyncWriteExt + Unpin>(mut tls: S, shared: &Shared) {
    let mut raw = Vec::new();
    let mut buffer = [0u8; 4096];
    while !raw.windows(4).any(|window| window == b"\r\n\r\n") {
        match tls.read(&mut buffer).await {
            Ok(read) if read > 0 => raw.extend_from_slice(&buffer[..read]),
            _ => return,
        }
    }
    let text = String::from_utf8_lossy(&raw).into_owned();
    let target = text
        .lines()
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .unwrap_or_default()
        .to_owned();
    let range = text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("range")
            .then(|| value.trim().to_owned())
    });
    let (status, extra, body) = answer(shared, &target, range.as_deref());
    held(&shared.hits).push(Hit {
        target,
        range,
        status,
    });
    let head = format!(
        "HTTP/1.1 {status} X\r\n{extra}Content-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _sent = tls.write_all(head.as_bytes()).await;
    let _sent = tls.write_all(&body).await;
    let _closed = tls.shutdown().await;
}

/// The status, extra header lines and body for `target`.
fn answer(shared: &Shared, target: &str, range: Option<&str>) -> (u16, String, Vec<u8>) {
    if let Some(to) = held(&shared.redirects).get(target) {
        return (302, format!("Location: {to}\r\n"), Vec::new());
    }
    let files = held(&shared.files);
    let Some(body) = files.get(target) else {
        return (404, String::new(), Vec::new());
    };
    let from = range
        .and_then(|range| range.strip_prefix("bytes="))
        .and_then(|range| range.strip_suffix('-'))
        .and_then(|start| start.parse::<usize>().ok());
    match from {
        None => (200, String::new(), body.clone()),
        Some(start) if start < body.len() => (
            206,
            format!(
                "Content-Range: bytes {start}-{}/{}\r\n",
                body.len() - 1,
                body.len()
            ),
            body[start..].to_vec(),
        ),
        Some(_) => (416, String::new(), Vec::new()),
    }
}
