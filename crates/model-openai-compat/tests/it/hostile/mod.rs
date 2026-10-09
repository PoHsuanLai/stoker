//! Model output is hostile input: the SSE framer and the stream decoder, driven with arbitrary
//! bytes, arbitrary chunk splits and structured mutations of valid replies. Every run ends in a
//! typed result, never a panic, and a tool call is reported only when it was read in full.

mod cases;
mod pipe;
mod props;
