//! Shared by the fuzz targets: the chunk-splitting prefix of an input, and the pipeline and
//! invariants the `hostile` proptest suite of model-openai-compat uses (included, not copied).

#[allow(dead_code)]
#[path = "../../crates/model-openai-compat/tests/hostile/pipe.rs"]
pub mod pipe;

/// An input is: one byte (how many cuts, 0 to 7), that many 16-bit cut points, then the payload
/// the targets decode. The same payload with other cuts must decode alike.
pub fn split(data: &[u8]) -> (Vec<usize>, &[u8]) {
    let Some((count, rest)) = data.split_first() else {
        return (Vec::new(), &[]);
    };
    let take = (usize::from(*count % 8) * 2).min(rest.len());
    let (head, payload) = rest.split_at(take);
    let cuts = head
        .chunks_exact(2)
        .map(|p| usize::from(u16::from_le_bytes([p[0], p[1]])))
        .collect();
    (cuts, payload)
}

/// `bytes` cut at `cuts` (each taken modulo the length).
pub fn pieces<'a>(bytes: &'a [u8], cuts: &[usize]) -> Vec<&'a [u8]> {
    let mut points: Vec<usize> = cuts.iter().map(|c| c % (bytes.len() + 1)).collect();
    points.extend([0, bytes.len()]);
    points.sort_unstable();
    points.windows(2).map(|w| &bytes[w[0]..w[1]]).collect()
}
