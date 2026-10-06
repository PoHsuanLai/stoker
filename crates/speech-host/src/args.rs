//! The command line: `--socket <path> --model-dir <dir> --threads <n> --chunk-ms <ms>`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use speech_provider::AudioMs;

/// Threads the recognizer may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadCount(pub u16);

/// What the command line says: `--socket <path> --model-dir <dir> --threads <n> --chunk-ms <ms>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostArgs {
    pub socket: PathBuf,
    /// The directory of ONNX files and `tokens.txt`.
    pub model_dir: PathBuf,
    pub threads: ThreadCount,
    pub chunk: AudioMs,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgsError {
    #[error("missing argument {0}")]
    Missing(&'static str),
    #[error("argument {0} is not a valid value")]
    Invalid(&'static str),
    #[error("unknown argument {0}")]
    Unknown(String),
}

const SOCKET: &str = "--socket";
const MODEL_DIR: &str = "--model-dir";
const THREADS: &str = "--threads";
const CHUNK_MS: &str = "--chunk-ms";

#[derive(Default)]
struct Seen {
    socket: Option<String>,
    model_dir: Option<String>,
    threads: Option<String>,
    chunk_ms: Option<String>,
}

impl Seen {
    /// Stores `value` in the slot of `flag`; a second value for the same flag is invalid.
    fn put(&mut self, flag: &str, value: String) -> Result<(), ArgsError> {
        let (name, slot) = match flag {
            SOCKET => (SOCKET, &mut self.socket),
            MODEL_DIR => (MODEL_DIR, &mut self.model_dir),
            THREADS => (THREADS, &mut self.threads),
            CHUNK_MS => (CHUNK_MS, &mut self.chunk_ms),
            other => return Err(ArgsError::Unknown(other.to_string())),
        };
        match slot {
            Some(_) => Err(ArgsError::Invalid(name)),
            None => {
                *slot = Some(value);
                Ok(())
            }
        }
    }
}

fn positive<T: TryFrom<u64> + PartialEq + Default>(
    name: &'static str,
    text: &str,
) -> Result<T, ArgsError> {
    let n: u64 = text.parse().map_err(|_| ArgsError::Invalid(name))?;
    let value = T::try_from(n).map_err(|_| ArgsError::Invalid(name))?;
    if value == T::default() {
        return Err(ArgsError::Invalid(name));
    }
    Ok(value)
}

fn path(name: &'static str, text: &str) -> Result<PathBuf, ArgsError> {
    if text.is_empty() {
        return Err(ArgsError::Invalid(name));
    }
    Ok(PathBuf::from(text))
}

/// Reads the arguments after the program name: the four flags, each exactly once, each followed
/// by its value, and nothing else.
pub fn parse_args(args: &[String]) -> Result<HostArgs, ArgsError> {
    let mut seen = Seen::default();
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        if !matches!(flag.as_str(), SOCKET | MODEL_DIR | THREADS | CHUNK_MS) {
            return Err(ArgsError::Unknown(flag.clone()));
        }
        let Some(value) = rest.next() else {
            return Err(ArgsError::Invalid(flag_name(flag)));
        };
        seen.put(flag, value.clone())?;
    }
    let need = |slot: Option<String>, name| slot.ok_or(ArgsError::Missing(name));
    let socket = need(seen.socket, SOCKET)?;
    let model_dir = need(seen.model_dir, MODEL_DIR)?;
    let threads = need(seen.threads, THREADS)?;
    let chunk = need(seen.chunk_ms, CHUNK_MS)?;
    Ok(HostArgs {
        socket: path(SOCKET, &socket)?,
        model_dir: path(MODEL_DIR, &model_dir)?,
        threads: ThreadCount(positive(THREADS, &threads)?),
        chunk: AudioMs(positive(CHUNK_MS, &chunk)?),
    })
}

fn flag_name(flag: &str) -> &'static str {
    match flag {
        SOCKET => SOCKET,
        MODEL_DIR => MODEL_DIR,
        THREADS => THREADS,
        _ => CHUNK_MS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    const GOOD: &str = "--socket /run/s.sock --model-dir /m --threads 6 --chunk-ms 560";

    #[test]
    fn the_four_flags_in_any_order() {
        let want = HostArgs {
            socket: "/run/s.sock".into(),
            model_dir: "/m".into(),
            threads: ThreadCount(6),
            chunk: AudioMs(560),
        };
        assert_eq!(parse_args(&argv(GOOD)), Ok(want.clone()));
        let shuffled = "--chunk-ms 560 --threads 6 --model-dir /m --socket /run/s.sock";
        assert_eq!(parse_args(&argv(shuffled)), Ok(want));
    }

    #[test]
    fn refused_lines() {
        let table: &[(&str, ArgsError)] = &[
            ("", ArgsError::Missing(SOCKET)),
            (
                "--socket /s --model-dir /m --threads 4",
                ArgsError::Missing(CHUNK_MS),
            ),
            (
                "--model-dir /m --threads 4 --chunk-ms 80",
                ArgsError::Missing(SOCKET),
            ),
            (
                "--socket /s --socket /t --model-dir /m --threads 4 --chunk-ms 80",
                ArgsError::Invalid(SOCKET),
            ),
            (
                "--socket /s --model-dir /m --threads 4 --threads 4 --chunk-ms 80",
                ArgsError::Invalid(THREADS),
            ),
            (
                "--socket /s --model-dir /m --threads x --chunk-ms 80",
                ArgsError::Invalid(THREADS),
            ),
            (
                "--socket /s --model-dir /m --threads 0 --chunk-ms 80",
                ArgsError::Invalid(THREADS),
            ),
            (
                "--socket /s --model-dir /m --threads 70000 --chunk-ms 80",
                ArgsError::Invalid(THREADS),
            ),
            (
                "--socket /s --model-dir /m --threads -1 --chunk-ms 80",
                ArgsError::Invalid(THREADS),
            ),
            (
                "--socket /s --model-dir /m --threads 4 --chunk-ms 0",
                ArgsError::Invalid(CHUNK_MS),
            ),
            (
                "--socket /s --model-dir /m --threads 4 --chunk-ms 80ms",
                ArgsError::Invalid(CHUNK_MS),
            ),
            (
                "--socket /s --model-dir /m --threads 4 --chunk-ms",
                ArgsError::Invalid(CHUNK_MS),
            ),
            (
                "--socket /s --model-dir /m --threads 4 --chunk-ms 80 --verbose",
                ArgsError::Unknown("--verbose".into()),
            ),
            (
                "stray --socket /s --model-dir /m --threads 4 --chunk-ms 80",
                ArgsError::Unknown("stray".into()),
            ),
            (
                "--socket=/s --model-dir /m --threads 4 --chunk-ms 80",
                ArgsError::Unknown("--socket=/s".into()),
            ),
        ];
        for (line, want) in table {
            assert_eq!(parse_args(&argv(line)).as_ref(), Err(want), "{line}");
        }
    }

    #[test]
    fn an_empty_path_is_invalid() {
        let line = vec![
            "--socket".to_string(),
            String::new(),
            "--model-dir".into(),
            "/m".into(),
            "--threads".into(),
            "4".into(),
            "--chunk-ms".into(),
            "80".into(),
        ];
        assert_eq!(parse_args(&line), Err(ArgsError::Invalid(SOCKET)));
    }
}
