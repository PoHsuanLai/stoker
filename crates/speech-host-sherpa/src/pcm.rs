//! PCM bytes to the f32 samples sherpa-onnx takes.

use speech_provider::PcmFormat;

/// Mono samples in [-1, 1]; a trailing partial sample is dropped.
pub fn to_f32(format: PcmFormat, bytes: &[u8]) -> Vec<f32> {
    match format {
        PcmFormat::S16Le => bytes
            .chunks_exact(2)
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
            .collect(),
        PcmFormat::F32Le => bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s16_scales_and_drops_a_trailing_byte() {
        let bytes = [0x00, 0x80, 0xff, 0x7f, 0x00, 0x00, 0x01];
        let got = to_f32(PcmFormat::S16Le, &bytes);
        assert_eq!(got, vec![-1.0, 32767.0 / 32768.0, 0.0]);
    }

    #[test]
    fn f32_passes_through() {
        let bytes: Vec<u8> = [0.5f32, -0.25]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        assert_eq!(to_f32(PcmFormat::F32Le, &bytes), vec![0.5, -0.25]);
    }
}
