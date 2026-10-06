//! `speech-host --socket <path> --model-dir <dir> --threads <n> --chunk-ms <ms>`

use std::process::ExitCode;

use speech_host::{HostError, parse_args, serve_with};
use speech_host_sherpa::SherpaRecognizer;

fn run(args: &[String]) -> Result<(), HostError> {
    let args = parse_args(args).map_err(|_| HostError::Model)?;
    let mut recognizer = SherpaRecognizer::load(&args)?;
    serve_with(&args, &mut recognizer)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if parse_args(&args).is_err() {
        return ExitCode::from(2);
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
