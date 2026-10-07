#!/usr/bin/env bash
# Fuzzes the decoders of model, engine and socket bytes on nightly with cargo-fuzz. A dev script,
# run by hand: the gate runs the `hostile` proptest suites instead. Needs a nightly toolchain
# (`rustup toolchain install nightly`) and `cargo install cargo-fuzz`; this script installs
# neither.
#
#   dev/fuzz.sh                    # every target, 1 minute each
#   dev/fuzz.sh 10                 # every target, 10 minutes each
#   dev/fuzz.sh 10 sse_stream      # one target, 10 minutes
#
# Targets: sse_stream (SSE framer + chat stream decoder), framers (SSE and NDJSON), whole_bodies
# (model list, embeddings, transcription, error replies), pcm, host_frames (speech-host wire).
# Seeds are in fuzz/corpus/<target>; a crash is written to fuzz/artifacts/<target>.
set -euo pipefail
cd "$(dirname "$0")/../fuzz"
export RUSTUP_TOOLCHAIN=nightly
rustup toolchain list | grep -q '^nightly' || { echo "no nightly toolchain installed" >&2; exit 2; }
cargo fuzz --version >/dev/null 2>&1 || { echo "cargo-fuzz is not installed" >&2; exit 2; }
seconds=$(( ${1:-1} * 60 ))
targets=(sse_stream framers whole_bodies pcm host_frames)
[ "$#" -ge 2 ] && targets=("$2")
for target in "${targets[@]}"; do
  cargo fuzz run "$target" -- -max_total_time="$seconds" -max_len=65536
done
