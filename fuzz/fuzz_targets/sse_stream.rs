//! SSE bytes through the framer and the chat stream decoder: no panic, a sound reply, and the
//! same verdict (and, for a clean end, the same events) whatever the chunking.
#![no_main]

use libfuzzer_sys::fuzz_target;
use stoker_fuzz::{pipe, split};

fuzz_target!(|data: &[u8]| {
    let (cuts, bytes) = split(data);
    let cut = pipe::run_cut(bytes, &cuts);
    pipe::assert_sound(&cut);
    let whole = pipe::run_cut(bytes, &[]);
    assert_eq!(cut.end.is_ok(), whole.end.is_ok());
    if cut.end.is_ok() {
        assert_eq!(cut, whole);
    }
});
