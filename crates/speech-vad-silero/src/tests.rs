use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use proptest::prelude::*;
use speech_provider::{Frame512, SpeechProb, VoiceActivity, Voiced};

use crate::core::{CONTEXT_SAMPLES, Detector, INPUT_SAMPLES, Infer, Input, State};
use crate::core::{thousandths, verdict};
use crate::{SileroConfig, SileroError, SileroModelPath, SileroVad};

#[derive(Debug, Default)]
struct Seen {
    inputs: Vec<Input>,
    states: Vec<State>,
}

/// Returns a fixed probability and a state that is the incoming one plus one; `None` on demand.
struct Fake {
    prob: f32,
    fail: bool,
    seen: Arc<Mutex<Seen>>,
}

impl Infer for Fake {
    fn run(&mut self, input: &Input, state: &State) -> Option<(f32, State)> {
        let mut seen = self.seen.lock().unwrap();
        seen.inputs.push(*input);
        seen.states.push(*state);
        if self.fail {
            return None;
        }
        Some((self.prob, state.map(|v| v + 1.0)))
    }
}

fn detector(prob: f32, threshold: u16) -> (Detector<Fake>, Arc<Mutex<Seen>>) {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let fake = Fake {
        prob,
        fail: false,
        seen: Arc::clone(&seen),
    };
    (Detector::new(fake, SpeechProb(threshold)), seen)
}

fn frame(fill: i16) -> Frame512 {
    Frame512::new(&[fill; 512]).unwrap()
}

#[test]
fn state_is_carried_from_frame_to_frame() {
    let (mut d, seen) = detector(0.5, 500);
    for _ in 0..3 {
        d.push(&frame(0));
    }
    let firsts: Vec<f32> = seen.lock().unwrap().states.iter().map(|s| s[0]).collect();
    assert_eq!(firsts, [0.0, 1.0, 2.0]);
}

#[test]
fn reset_zeroes_state_and_context() {
    let (mut d, seen) = detector(0.5, 500);
    d.push(&frame(16384));
    d.push(&frame(16384));
    d.reset();
    d.push(&frame(0));
    let seen = seen.lock().unwrap();
    assert!(seen.states[2].iter().all(|v| *v == 0.0));
    assert!(seen.inputs[2][..CONTEXT_SAMPLES].iter().all(|v| *v == 0.0));
}

#[test]
fn context_is_the_tail_of_the_previous_frame() {
    let (mut d, seen) = detector(0.5, 500);
    let ramp: Vec<i16> = (0..512).collect();
    d.push(&Frame512::new(&ramp).unwrap());
    d.push(&frame(0));
    let seen = seen.lock().unwrap();
    assert!(seen.inputs[0][..CONTEXT_SAMPLES].iter().all(|v| *v == 0.0));
    let tail: Vec<f32> = (448_i16..512).map(|v| f32::from(v) / 32768.0).collect();
    assert_eq!(&seen.inputs[1][..CONTEXT_SAMPLES], tail.as_slice());
    assert_eq!(seen.inputs[0].len(), INPUT_SAMPLES);
    assert_eq!(seen.inputs[0][CONTEXT_SAMPLES + 3], 3.0 / 32768.0);
}

#[test]
fn samples_are_scaled_to_unit_range() {
    let (mut d, seen) = detector(0.5, 500);
    d.push(&frame(i16::MIN));
    assert_eq!(seen.lock().unwrap().inputs[0][INPUT_SAMPLES - 1], -1.0);
}

#[test]
fn a_failed_run_is_silence_and_keeps_the_state() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let mut d = Detector::new(
        Fake {
            prob: 0.9,
            fail: false,
            seen: Arc::clone(&seen),
        },
        SpeechProb(500),
    );
    d.push(&frame(0));
    d.infer.fail = true;
    assert_eq!(d.push(&frame(0)), (Voiced::Silence, SpeechProb(0)));
    d.infer.fail = false;
    d.push(&frame(0));
    assert_eq!(seen.lock().unwrap().states[2][0], 1.0);
}

#[test]
fn push_reports_probability_and_verdict() {
    let (mut d, _) = detector(0.8125, 500);
    assert_eq!(d.push(&frame(0)), (Voiced::Speech, SpeechProb(813)));
    let (mut d, _) = detector(0.25, 500);
    assert_eq!(d.push(&frame(0)), (Voiced::Silence, SpeechProb(250)));
}

#[test]
fn thousandths_table() {
    let table = [
        (0.0, 0),
        (0.0004, 0),
        (0.0005, 1),
        (0.5, 500),
        (0.9996, 1000),
        (1.0, 1000),
        (1.7, 1000),
        (-0.2, 0),
        (f32::NAN, 0),
        (f32::INFINITY, 1000),
    ];
    for (p, want) in table {
        assert_eq!(thousandths(p), SpeechProb(want), "{p}");
    }
}

#[test]
fn threshold_is_inclusive() {
    let table = [
        (499, 500, Voiced::Silence),
        (500, 500, Voiced::Speech),
        (501, 500, Voiced::Speech),
        (0, 0, Voiced::Speech),
    ];
    for (p, t, want) in table {
        assert_eq!(verdict(SpeechProb(p), SpeechProb(t)), want, "{p} vs {t}");
    }
}

proptest! {
    #[test]
    fn probability_never_exceeds_one_thousand(p in any::<f32>()) {
        prop_assert!(thousandths(p) <= SpeechProb(1000));
    }

    #[test]
    fn one_run_per_push(n in 1usize..8) {
        let (mut d, seen) = detector(0.5, 500);
        for _ in 0..n { d.push(&frame(1)); }
        prop_assert_eq!(seen.lock().unwrap().states.len(), n);
    }
}

#[test]
fn a_missing_model_file_is_model_missing() {
    let config = SileroConfig {
        model: SileroModelPath(PathBuf::from("/nonexistent/silero_vad.onnx")),
        threshold: SpeechProb(500),
    };
    assert_eq!(
        SileroVad::load(&config).unwrap_err(),
        SileroError::ModelMissing
    );
}

#[test]
fn a_file_that_is_not_a_model_is_a_runtime_error() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let config = SileroConfig {
        model: SileroModelPath(path),
        threshold: SpeechProb(500),
    };
    assert!(matches!(
        SileroVad::load(&config),
        Err(SileroError::Runtime(_))
    ));
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/audio")
        .join(name)
}

fn wav_frames() -> Vec<Frame512> {
    let bytes = std::fs::read(fixture("silero_synth.wav")).unwrap();
    let pcm: Vec<i16> = bytes[44..]
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    pcm.chunks_exact(512)
        .map(|c| Frame512::new(c).unwrap())
        .collect()
}

fn reference() -> Vec<f32> {
    let text = std::fs::read_to_string(fixture("silero_synth_ref.csv")).unwrap();
    text.lines()
        .skip(1)
        .map(|l| l.split(',').nth(1).unwrap().parse().unwrap())
        .collect()
}

#[test]
fn the_fixture_is_whole_frames_with_one_reference_row_each() {
    assert_eq!(wav_frames().len(), reference().len());
}

/// Runs the real model over the synthetic fixture and compares with the onnxruntime reference.
#[test]
#[ignore = "needs silero_vad.onnx v6.2.1 (not in the repository): STOKER_SILERO_ONNX=<path>, run with --ignored"]
fn matches_the_onnxruntime_reference_within_a_thousandth() {
    let path =
        std::env::var("STOKER_SILERO_ONNX").expect("set STOKER_SILERO_ONNX to silero_vad.onnx");
    let config = SileroConfig {
        model: SileroModelPath(PathBuf::from(path)),
        threshold: SpeechProb(500),
    };
    let started = std::time::Instant::now();
    let mut vad = SileroVad::load(&config).unwrap();
    eprintln!("load time: {:?}", started.elapsed());
    let run_started = std::time::Instant::now();
    let mut worst = 0.0_f32;
    for (i, (frame, want)) in wav_frames().iter().zip(reference()).enumerate() {
        let (_, SpeechProb(got)) = vad.push(frame);
        // The detector reports thousandths, so a half-thousandth of rounding is allowed on top.
        let diff = (f32::from(got) / 1000.0 - want).abs();
        worst = worst.max(diff);
        assert!(diff <= 1e-3 + 5e-4, "frame {i}: got {got}, want {want}");
    }
    eprintln!(
        "max abs diff: {worst}; per frame: {:?}",
        run_started.elapsed() / 118
    );
    vad.reset();
    let first = vad.push(&wav_frames()[0]);
    assert_eq!(
        first.1,
        SpeechProb((reference()[0] * 1000.0).round() as u16)
    );
}
