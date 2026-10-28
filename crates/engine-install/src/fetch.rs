//! One pinned file from an `https` address to a path: resumable, cancellable, counted, and
//! kept only if its size and SHA-256 are the pinned ones.
//!
//! The bytes go to `<dest>.part` beside the destination. A broken connection is picked up again
//! with a `Range` request from the bytes already there (the file is hashed first, so the
//! checksum covers all of it). A file that is complete but wrong is deleted. A stop or a lost
//! connection that outlasts the retries leaves the part for the next try.

use crate::pinned_url::PinnedUrl;
use crate::progress::{CancelToken, Meter};
use engine_manifest::{Artifact, hex};
use engine_probe::file_digest;
use model_http::{
    AuthHeader, BodySink, ChunkFlow, Exchange, ExtraHeader, Framing, HeaderName, HostName,
    HttpClient, HttpEndpoint, HttpError, HttpTarget, Port, Proxy, ResponseHead, RouteRoot, Secret,
    Timeouts, TlsRoots, Transport, UrlPath, Verb, WaitMs,
};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How many redirects one download follows.
const MAX_REDIRECTS: usize = 5;

/// Why a download did not produce its file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum FetchError {
    /// The connection failed or broke off, and the retries ran out.
    #[error("the download could not be finished: {0}")]
    Network(HttpError),
    /// The server answered with this status.
    #[error("the server answered {0}")]
    Refused(u16),
    /// What arrived is not what is pinned; it was deleted.
    #[error("what arrived is not what is pinned")]
    Corrupt,
    /// The disk refused the bytes.
    #[error("the disk refused the download: {0}")]
    Disk(std::io::ErrorKind),
    /// A stop was asked for.
    #[error("stopped")]
    Cancelled,
    /// An address that is not `https`, or redirects that lead nowhere.
    #[error("the address is not an https one, or the redirects lead nowhere")]
    BadAddress,
}

/// Downloads pinned files.
#[derive(Debug, Clone)]
pub struct Fetcher {
    roots: TlsRoots,
    idle: Duration,
    retries: u32,
    pause: Duration,
}

impl Fetcher {
    /// A fetcher that trusts the system's certificate authorities, with the usual waits.
    pub fn system() -> Self {
        Self::new(TlsRoots::Platform)
    }

    /// A fetcher whose servers must chain to `roots`: 30 seconds of silence is a lost
    /// connection, and a lost connection is retried three times, two seconds apart.
    pub fn new(roots: TlsRoots) -> Self {
        Self {
            roots,
            idle: Duration::from_secs(30),
            retries: 3,
            pause: Duration::from_secs(2),
        }
    }

    /// The same, with these waits and this many retries (a test's own).
    #[must_use]
    pub fn with_timing(self, idle: Duration, retries: u32, pause: Duration) -> Self {
        Self {
            idle,
            retries,
            pause,
            ..self
        }
    }

    /// Brings `artifact` to `dest`. A `dest` that already holds the pinned file is kept as it is.
    /// `meter` gains the file's bytes as they arrive.
    ///
    /// # Errors
    /// See [`FetchError`].
    pub async fn fetch(
        &self,
        artifact: &Artifact,
        dest: &Path,
        meter: &Meter,
        cancel: &CancelToken,
    ) -> Result<(), FetchError> {
        let base = meter.done();
        if file_digest(dest)
            .is_ok_and(|(size, hash)| size == artifact.size && hash == artifact.sha256)
        {
            meter.set_done(base + artifact.size);
            return Ok(());
        }
        let _stale = std::fs::remove_file(dest);
        let part = part_of(dest);
        let mut attempt = 0;
        loop {
            match self.once(artifact, &part, base, meter, cancel).await {
                Ok(()) => break,
                Err(FetchError::Network(_)) if attempt < self.retries && !cancel.is_cancelled() => {
                    attempt += 1;
                    tokio::select! {
                        () = tokio::time::sleep(self.pause) => {}
                        () = cancel.cancelled() => return Err(FetchError::Cancelled),
                    }
                }
                Err(other) => return Err(other),
            }
        }
        std::fs::rename(&part, dest).map_err(|e| FetchError::Disk(e.kind()))
    }

    /// The endpoint of one request to `url`; `have` is how many bytes are already in the part.
    fn endpoint(&self, url: &PinnedUrl, have: u64) -> HttpEndpoint {
        let wait = WaitMs(u32::try_from(self.idle.as_millis()).unwrap_or(u32::MAX));
        let range = (have > 0).then(|| ExtraHeader {
            name: HeaderName("range".to_owned()),
            value: Secret(format!("bytes={have}-")),
        });
        HttpEndpoint {
            target: HttpTarget::Tls {
                host: HostName(url.host().to_owned()),
                port: Port(url.port()),
            },
            proxy: Proxy::Direct,
            base: UrlPath(String::new()),
            auth: AuthHeader::None,
            headers: range.into_iter().collect(),
            timeouts: Timeouts {
                connect: wait,
                first_byte: wait,
                idle: wait,
            },
        }
    }

    async fn once(
        &self,
        artifact: &Artifact,
        part: &Path,
        base: u64,
        meter: &Meter,
        cancel: &CancelToken,
    ) -> Result<(), FetchError> {
        let mut url = PinnedUrl::parse(artifact.url).map_err(|_| FetchError::BadAddress)?;
        for _hop in 0..=MAX_REDIRECTS {
            let mut sink = Sink::resume(part, artifact.size, base, meter, cancel)?;
            let client = HttpClient::with_roots(self.endpoint(&url, sink.have), self.roots.clone());
            let asked = Exchange {
                verb: Verb::Get,
                root: RouteRoot::Server,
                path: UrlPath(url.path_and_query()),
                body: None,
                framing: Framing::Whole,
            };
            let sent = tokio::select! {
                () = cancel.cancelled() => None,
                sent = client.exchange(&asked, &mut sink) => Some(sent),
            };
            let Some(sent) = sent else {
                return Err(FetchError::Cancelled);
            };
            match (sent, sink.status) {
                (Ok(_), 200 | 206) => return sink.complete(artifact, part),
                (Err(HttpError::Rejected), 301 | 302 | 303 | 307 | 308) => {
                    let location = sink.location.take().ok_or(FetchError::BadAddress)?;
                    url = url.join(&location).map_err(|_| FetchError::BadAddress)?;
                }
                (Err(HttpError::Rejected), 416) => {
                    // The part is longer than the file or not a part of it: start again.
                    let _gone = std::fs::remove_file(part);
                    meter.set_done(base);
                }
                (Err(HttpError::Rejected), other) | (Ok(_), other) => {
                    return Err(FetchError::Refused(other));
                }
                (Err(other), _) => return Err(FetchError::Network(other)),
            }
        }
        Err(FetchError::BadAddress)
    }
}

/// `dest` with `.part` after its name.
fn part_of(dest: &Path) -> PathBuf {
    let mut name: OsString = dest.file_name().map(OsString::from).unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
}

/// Receives one response into the part file.
struct Sink {
    file: File,
    hasher: Sha256,
    /// Bytes in the file now.
    have: u64,
    expect: u64,
    base: u64,
    meter: Meter,
    cancel: CancelToken,
    writing: bool,
    /// The status of the response head, 0 until it is seen.
    status: u16,
    /// The `Location` of a redirect.
    location: Option<String>,
    failed: Option<std::io::ErrorKind>,
    overrun: bool,
    stopped: bool,
    misplaced: bool,
}

impl Sink {
    /// Opens the part file (making it) and hashes what is already in it; a part longer than the
    /// file is emptied.
    fn resume(
        part: &Path,
        expect: u64,
        base: u64,
        meter: &Meter,
        cancel: &CancelToken,
    ) -> Result<Self, FetchError> {
        let disk = |e: std::io::Error| FetchError::Disk(e.kind());
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(part)
            .map_err(disk)?;
        let mut sink = Self {
            hasher: Sha256::new(),
            have: 0,
            file: file.try_clone().map_err(disk)?,
            expect,
            base,
            meter: meter.clone(),
            cancel: cancel.clone(),
            writing: false,
            status: 0,
            location: None,
            failed: None,
            overrun: false,
            stopped: false,
            misplaced: false,
        };
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer).map_err(disk)?;
            if read == 0 {
                break;
            }
            sink.hasher.update(&buffer[..read]);
            sink.have += read as u64;
        }
        if sink.have > expect {
            sink.start_over()?;
        }
        meter.set_done(base + sink.have);
        Ok(sink)
    }

    fn start_over(&mut self) -> Result<(), FetchError> {
        self.file
            .set_len(0)
            .map_err(|e| FetchError::Disk(e.kind()))?;
        self.hasher = Sha256::new();
        self.have = 0;
        self.meter.set_done(self.base);
        Ok(())
    }

    /// What the finished response comes to.
    fn complete(self, artifact: &Artifact, part: &Path) -> Result<(), FetchError> {
        if let Some(kind) = self.failed {
            return Err(FetchError::Disk(kind));
        }
        if self.stopped {
            return Err(FetchError::Cancelled);
        }
        let wrong = self.overrun
            || self.misplaced
            || (self.have == artifact.size && hex(&self.hasher.finalize()) != artifact.sha256);
        if wrong {
            let _gone = std::fs::remove_file(part);
            return Err(FetchError::Corrupt);
        }
        if self.have < artifact.size {
            // The connection ended early: the part stays and the next request resumes it.
            return Err(FetchError::Network(HttpError::Broken));
        }
        Ok(())
    }
}

impl BodySink for Sink {
    fn head(&mut self, head: &ResponseHead) -> ChunkFlow {
        self.status = head.status.0;
        match self.status {
            301 | 302 | 303 | 307 | 308 => {
                self.location.clone_from(&head.location);
                ChunkFlow::Stop
            }
            200 => {
                // The server sent the whole file whatever was asked: begin again.
                if self.have > 0 && self.start_over().is_err() {
                    self.failed = Some(std::io::ErrorKind::Other);
                    return ChunkFlow::Stop;
                }
                self.writing = true;
                ChunkFlow::Continue
            }
            206 => {
                let wanted = format!("bytes {}-", self.have);
                self.misplaced = !head
                    .content_range
                    .as_deref()
                    .is_some_and(|range| range.starts_with(&wanted));
                self.writing = !self.misplaced;
                if self.writing {
                    ChunkFlow::Continue
                } else {
                    ChunkFlow::Stop
                }
            }
            _ => ChunkFlow::Stop,
        }
    }

    fn chunk(&mut self, bytes: &[u8]) -> ChunkFlow {
        if !self.writing {
            return ChunkFlow::Stop;
        }
        if self.cancel.is_cancelled() {
            self.stopped = true;
            return ChunkFlow::Stop;
        }
        if let Err(error) = self.file.write_all(bytes) {
            self.failed = Some(error.kind());
            return ChunkFlow::Stop;
        }
        self.hasher.update(bytes);
        self.have += bytes.len() as u64;
        self.meter.add(bytes.len() as u64);
        if self.have > self.expect {
            self.overrun = true;
            return ChunkFlow::Stop;
        }
        ChunkFlow::Continue
    }
}
