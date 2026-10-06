//! The model-free part: context, recurrent state, probability mapping and thresholding.

use speech_provider::{FRAME_SAMPLES, Frame512, SpeechProb, Voiced};

/// Samples of the previous frame prepended to the next one (16 kHz).
pub const CONTEXT_SAMPLES: usize = 64;
/// Model input length: context plus frame.
pub const INPUT_SAMPLES: usize = CONTEXT_SAMPLES + FRAME_SAMPLES;
/// Elements of the [2, 1, 128] recurrent state.
pub const STATE_LEN: usize = 2 * 128;

pub type State = [f32; STATE_LEN];
pub type Input = [f32; INPUT_SAMPLES];

/// One model step: input (context + frame) and state in, probability and next state out.
/// A failed run is reported as `None`; the detector then says silence and keeps its state.
pub trait Infer: Send {
    fn run(&mut self, input: &Input, state: &State) -> Option<(f32, State)>;
}

#[derive(Debug)]
pub struct Detector<I> {
    pub(crate) infer: I,
    threshold: SpeechProb,
    pub(crate) state: State,
    context: [f32; CONTEXT_SAMPLES],
}

impl<I: Infer> Detector<I> {
    pub fn new(infer: I, threshold: SpeechProb) -> Self {
        Self {
            infer,
            threshold,
            state: [0.0; STATE_LEN],
            context: [0.0; CONTEXT_SAMPLES],
        }
    }

    pub fn push(&mut self, frame: &Frame512) -> (Voiced, SpeechProb) {
        let mut input = [0.0_f32; INPUT_SAMPLES];
        input[..CONTEXT_SAMPLES].copy_from_slice(&self.context);
        for (slot, s) in input[CONTEXT_SAMPLES..].iter_mut().zip(frame.samples()) {
            *slot = f32::from(*s) / 32768.0;
        }
        self.context
            .copy_from_slice(&input[INPUT_SAMPLES - CONTEXT_SAMPLES..]);
        let Some((p, next)) = self.infer.run(&input, &self.state) else {
            return (Voiced::Silence, SpeechProb(0));
        };
        self.state = next;
        let prob = thousandths(p);
        (verdict(prob, self.threshold), prob)
    }

    pub fn reset(&mut self) {
        self.state = [0.0; STATE_LEN];
        self.context = [0.0; CONTEXT_SAMPLES];
    }
}

/// A probability in [0, 1] as thousandths, rounded; NaN and out-of-range values clamp.
pub fn thousandths(p: f32) -> SpeechProb {
    let t = (p * 1000.0).round();
    // The clamp bounds the value to 0..=1000, so the cast cannot truncate.
    SpeechProb(if t.is_nan() {
        0
    } else {
        t.clamp(0.0, 1000.0) as u16
    })
}

pub fn verdict(prob: SpeechProb, threshold: SpeechProb) -> Voiced {
    if prob >= threshold {
        Voiced::Speech
    } else {
        Voiced::Silence
    }
}
