//! The socket: length-prefixed JSON frames in and out, one connection at a time.

use std::io::{self, Read, Write};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::Path;

use model_provider::ProviderError;
use speech_provider::{HostIn, HostOut, decode_frame, encode_frame, frame_length};

use crate::{HostArgs, HostError, Recognizer, Session};

/// Binds `path` with owner-only permissions; a stale socket file at the path is replaced, any
/// other kind of file is left alone and refused.
pub fn bind(path: &Path) -> Result<UnixListener, HostError> {
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if !meta.file_type().is_socket() {
            return Err(HostError::Bind);
        }
        std::fs::remove_file(path).map_err(|_| HostError::Bind)?;
    }
    let listener = UnixListener::bind(path).map_err(|_| HostError::Bind)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| HostError::Bind)?;
    Ok(listener)
}

/// Binds `args.socket` and serves `recognizer` until the process is stopped.
pub fn serve_with<R: Recognizer>(args: &HostArgs, recognizer: &mut R) -> Result<(), HostError> {
    let listener = bind(&args.socket)?;
    serve_listener(&listener, recognizer)
}

/// Serves connections one after another: one utterance at a time across the whole process. A
/// connection that fails ends that connection only.
pub fn serve_listener<R: Recognizer>(
    listener: &UnixListener,
    recognizer: &mut R,
) -> Result<(), HostError> {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        // A broken connection (peer gone, bad frame) is that client's problem; the next one is served.
        let _ = serve_connection(recognizer, &mut stream);
    }
    Ok(())
}

/// Serves one connection until the peer closes it.
pub fn serve_connection<R: Recognizer, S: Read + Write>(
    recognizer: &mut R,
    stream: &mut S,
) -> io::Result<()> {
    let mut session = Session::new(recognizer);
    let result = pump(&mut session, stream);
    session.close();
    result
}

fn pump<R: Recognizer, S: Read + Write>(
    session: &mut Session<'_, R>,
    stream: &mut S,
) -> io::Result<()> {
    loop {
        let Some(body) = read_frame(stream)? else {
            return Ok(());
        };
        let replies = match decode_frame::<HostIn>(&body) {
            Ok(message) => session.step(message),
            Err(_) => vec![HostOut::Failed(ProviderError::BadRequest(
                "the frame is not a host message".to_string(),
            ))],
        };
        for reply in &replies {
            write_frame(stream, reply)?;
        }
    }
}

/// One frame's body; `None` at a clean end of stream between frames. A frame over the cap is
/// refused before its body is read, and the connection ends.
fn read_frame<S: Read>(stream: &mut S) -> io::Result<Option<Vec<u8>>> {
    let mut header = [0u8; 4];
    let mut got = 0;
    while got < header.len() {
        match stream.read(&mut header[got..])? {
            0 if got == 0 => return Ok(None),
            0 => return Err(io::ErrorKind::UnexpectedEof.into()),
            n => got += n,
        }
    }
    let len = frame_length(header).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body)?;
    Ok(Some(body))
}

fn write_frame<S: Write>(stream: &mut S, reply: &HostOut) -> io::Result<()> {
    let frame = encode_frame(reply).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    stream.write_all(&frame)?;
    stream.flush()
}
