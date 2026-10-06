#!/usr/bin/env bash
# By-hand check of speech-host with the Nemotron model: transcribes WAVs through the real host
# over its Unix socket and prints first-partial latency, release-to-final and real-time factor
# per thread count. CPU only; no microphone, speaker or network.
#
#   dev/speech-host-try.sh [--threads 4,6,8] [wav ...]      (default: the model's en.wav, zh.wav)
#
# Needs dev/build-sherpa.sh run once (cache under ~/rs-wt/v-host/cache by default; override with
# CACHE=...), the model in $CACHE/model, and uv (stdlib-only client).
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
cache=${CACHE:-$HOME/rs-wt/v-host/cache}
model=${MODEL_DIR:-$cache/model}
export SHERPA_ONNX_LIB_DIR=$cache/install/lib
export LD_LIBRARY_PATH=$SHERPA_ONNX_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
cargo build --release --manifest-path "$here/crates/speech-host-sherpa/Cargo.toml"
target=${CARGO_TARGET_DIR:-$here/crates/speech-host-sherpa/target}
threads=4,6,8
if [ "${1:-}" = --threads ]; then threads=$2; shift 2; fi
if [ $# -eq 0 ]; then set -- "$model/test_wavs/en.wav" "$model/test_wavs/zh.wav"; fi
exec uv run --no-project python -I "$here/dev/speech-host-try.py" \
  --host "$target/release/speech-host" --model-dir "$model" --threads "$threads" "$@"
