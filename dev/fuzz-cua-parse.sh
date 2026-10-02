#!/usr/bin/env bash
# Fuzzes cua-parse on nightly with cargo-fuzz (QUESTIONS P11). A dev script, run by hand; the
# stable proptest suite (`never_panics`) is what CI runs. Needs `cargo install cargo-fuzz` and a
# nightly toolchain.
#
#   dev/fuzz-cua-parse.sh                 # both targets, 60 s each
#   dev/fuzz-cua-parse.sh parse_text 600  # one target, 600 s
set -euo pipefail
cd "$(dirname "$0")/../crates/cua-parse"
export RUSTUP_TOOLCHAIN=nightly
seconds="${2:-60}"
targets=(parse_text parse_tool_calls)
[ "$#" -ge 1 ] && targets=("$1")
for target in "${targets[@]}"; do
  cargo fuzz run "$target" -- -max_total_time="$seconds" -max_len=40000
done
