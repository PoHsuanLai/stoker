//! The pure parts of voice activity: loudness, the energy gate, the framer and the endpointer,
//! and the three of them chained over synthetic audio.

use proptest::prelude::*;
use speech_provider::{
    AudioChunk, AudioFormat, AudioMs, Frame512, PcmBytes, PcmFormat, SampleIndex, SampleRate,
    VoiceActivity, Voiced,
};
use speech_vad::{
    EndWhy, Endpoint, EndpointParams, EnergyGate, EnergyGateParams, FrameCount, Framer,
    FramerError, Level, endpoint, level_of,
};

const S16: AudioFormat = AudioFormat {
    rate: SampleRate(16_000),
    pcm: PcmFormat::S16Le,
};

fn constant(a: i16) -> Frame512 {
    Frame512::new(&[a; 512]).unwrap()
}

fn bytes(samples: &[i16]) -> Vec<u8> {
    samples.iter().flat_map(|s| s.to_le_bytes()).collect()
}

fn chunk(at: u64, pcm: Vec<u8>) -> AudioChunk {
    AudioChunk {
        format: S16,
        at: SampleIndex(at),
        pcm: PcmBytes::new(pcm),
    }
}

#[test]
fn level_of_bounds() {
    // Constant frames; the expected levels are (60 + 20 log10(a / 32768)) / 60 * 1000 rounded,
    // from a float reference computed once offline (none of these sits near a rounding edge).
    const ROWS: &[(i16, u16)] = &[
        (0, 0),
        (1, 0),
        (32, 0),
        (33, 1),
        (327, 333),
        (328, 333),
        (3277, 667),
        (8192, 799),
        (16384, 900),
        (32767, 1000),
        (-32768, 1000),
        (-16384, 900),
    ];
    for (a, level) in ROWS {
        assert_eq!(level_of(&constant(*a)), Level(*level), "amplitude {a}");
    }
    // Silence is 0 and full scale is 1000, whatever the wave.
    let square: Vec<i16> = (0..512)
        .map(|i| if i % 2 == 0 { i16::MAX } else { i16::MIN })
        .collect();
    assert_eq!(level_of(&Frame512::new(&square).unwrap()), Level(1000));
    // One loud sample in a quiet frame: 16384^2 of 512 * 32768^2 is -33.1 dBFS.
    let mut one = [0_i16; 512];
    one[7] = 16384;
    assert_eq!(level_of(&Frame512::new(&one).unwrap()), Level(448));
}

proptest! {
    #[test]
    fn a_louder_frame_is_never_a_lower_level(a in 0_i16..=i16::MAX, b in 0_i16..=i16::MAX) {
        let (low, high) = (a.min(b), a.max(b));
        prop_assert!(level_of(&constant(low)) <= level_of(&constant(high)));
        prop_assert!(level_of(&constant(high)) <= Level(1000));
    }
}

fn gate(threshold: Level, hangover: u16) -> EnergyGate {
    EnergyGate::new(EnergyGateParams {
        threshold,
        hangover: FrameCount(hangover),
    })
}

#[test]
fn energy_gate_table() {
    let (loud, quiet) = (constant(3000), constant(0));
    let mut g = gate(level_of(&loud), 2);
    let run = |g: &mut EnergyGate, frames: &[&Frame512]| -> Vec<(Voiced, u16)> {
        frames
            .iter()
            .map(|f| {
                let (v, p) = g.push(f);
                (v, p.0)
            })
            .collect()
    };
    let s = (Voiced::Speech, 1000);
    let n = (Voiced::Silence, 0);
    // A frame at the threshold is loud; two quiet frames after the last loud one are still
    // speech; the third is silence, and a new loud frame starts the hold again.
    let got = run(
        &mut g,
        &[
            &quiet, &loud, &quiet, &quiet, &quiet, &quiet, &loud, &loud, &quiet,
        ],
    );
    assert_eq!(got, [n, s, s, s, n, n, s, s, s]);
    // A frame just under the threshold is quiet.
    let mut g = gate(level_of(&loud), 0);
    assert_eq!(run(&mut g, &[&constant(2900), &loud]), [n, s]);
    // Reset forgets the hold.
    let mut g = gate(level_of(&loud), 5);
    run(&mut g, &[&loud]);
    g.reset();
    assert_eq!(run(&mut g, &[&quiet]), [n]);
    // Hangover zero is the bare threshold; the default gate is -40 dBFS with 8 frames.
    assert_eq!(EnergyGateParams::default().threshold, Level(333));
}

#[test]
fn the_framer_cuts_whole_frames_and_numbers_them() {
    let samples: Vec<i16> = (0..1300).map(|i| i as i16).collect();
    let mut framer = Framer::new();
    let first = framer.push(&chunk(0, bytes(&samples[..1000]))).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!((first[0].at, framer.pending()), (SampleIndex(0), 488));
    assert_eq!(first[0].frame.samples()[..], samples[..512]);
    let second = framer.push(&chunk(1000, bytes(&samples[1000..]))).unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!((second[0].at, framer.pending()), (SampleIndex(512), 276));
    assert_eq!(second[0].frame.samples()[..], samples[512..1024]);
    // Nothing in, nothing out.
    assert_eq!(framer.push(&chunk(1300, vec![])).unwrap(), vec![]);
}

#[test]
fn the_framer_refuses_other_formats_and_joins_a_split_sample() {
    let mut framer = Framer::new();
    for format in [
        AudioFormat {
            rate: SampleRate(48_000),
            pcm: PcmFormat::S16Le,
        },
        AudioFormat {
            rate: SampleRate(16_000),
            pcm: PcmFormat::F32Le,
        },
    ] {
        let wrong = AudioChunk {
            format,
            at: SampleIndex(0),
            pcm: PcmBytes::new(vec![0; 4]),
        };
        assert_eq!(framer.push(&wrong), Err(FramerError::WrongFormat));
    }
    // The 512 samples arrive as one byte, then the rest: the split sample is not lost.
    let all = bytes(&[0x0102; 512]);
    assert_eq!(framer.push(&chunk(0, all[..1].to_vec())).unwrap(), vec![]);
    let frames = framer.push(&chunk(0, all[1..].to_vec())).unwrap();
    assert_eq!(frames.len(), 1);
    assert!(frames[0].frame.samples().iter().all(|s| *s == 0x0102));
}

#[test]
fn a_gap_in_the_audio_is_a_gap_in_the_numbers() {
    let mut framer = Framer::new();
    framer.push(&chunk(0, bytes(&[1; 512]))).unwrap();
    let after = framer.push(&chunk(5000, bytes(&[2; 512]))).unwrap();
    assert_eq!(after[0].at, SampleIndex(5000));
}

proptest! {
    #[test]
    fn framer_carries_remainder(
        samples in proptest::collection::vec(any::<i16>(), 0..3000),
        cuts in proptest::collection::vec(any::<usize>(), 0..8),
    ) {
        let pcm = bytes(&samples);
        let whole = Framer::new().push(&chunk(0, pcm.clone())).unwrap();
        // Cut the same bytes at arbitrary points, odd ones included.
        let mut points: Vec<usize> = cuts.iter().map(|c| c % (pcm.len() + 1)).collect();
        points.sort_unstable();
        let mut framer = Framer::new();
        let mut frames = Vec::new();
        let mut start = 0;
        for end in points.into_iter().chain([pcm.len()]) {
            frames.extend(framer.push(&chunk((start / 2) as u64, pcm[start..end].to_vec())).unwrap());
            start = end;
        }
        prop_assert_eq!(frames, whole);
        prop_assert_eq!(framer.pending(), samples.len() % 512);
    }
}

fn params() -> EndpointParams {
    EndpointParams {
        lead: AudioMs(0),
        silence_end: AudioMs(100),
        min_speech: AudioMs(64),
    }
}

/// States after each frame of 512 samples, from `Waiting`.
fn run(verdicts: &[Voiced]) -> Vec<Endpoint> {
    let mut state = Endpoint::Waiting;
    verdicts
        .iter()
        .enumerate()
        .map(|(i, v)| {
            state = endpoint(state, *v, SampleIndex(i as u64 * 512), &params());
            state
        })
        .collect()
}

use Voiced::{Silence as N, Speech as S};

fn at(n: u64) -> SampleIndex {
    SampleIndex(n * 512)
}

#[test]
fn endpoint_table() {
    // Only silence: waiting until `silence_end` (1600 samples) has passed, then NoSpeech.
    assert_eq!(
        run(&[N, N, N, N, N]),
        [
            Endpoint::Waiting,
            Endpoint::Waiting,
            Endpoint::Waiting,
            Endpoint::Waiting,
            Endpoint::Ended(EndWhy::NoSpeech)
        ]
    );
    // Speech, then silence: trailing from where the silence began, ended once it lasts 1600.
    assert_eq!(
        run(&[S, S, S, S, N, N, N, N, N]),
        [
            Endpoint::InSpeech { since: at(0) },
            Endpoint::InSpeech { since: at(0) },
            Endpoint::InSpeech { since: at(0) },
            Endpoint::InSpeech { since: at(0) },
            Endpoint::Trailing { since: at(4) },
            Endpoint::Trailing { since: at(4) },
            Endpoint::Trailing { since: at(4) },
            Endpoint::Trailing { since: at(4) },
            Endpoint::Ended(EndWhy::Silence)
        ]
    );
    // Speech starts later: `since` is its first frame.
    assert_eq!(run(&[N, N, S])[2], Endpoint::InSpeech { since: at(2) });
}

#[test]
fn a_burst_shorter_than_min_speech_is_not_speech() {
    // One frame (512 samples) is under 64 ms (1024): back to waiting, and the clock for
    // `NoSpeech` is the utterance's, not the burst's.
    let states = run(&[S, N, N, N, N]);
    assert_eq!(states[1], Endpoint::Waiting);
    assert_eq!(states[4], Endpoint::Ended(EndWhy::NoSpeech));
    // Two frames are 1024 samples: long enough.
    assert_eq!(run(&[S, S, N])[2], Endpoint::Trailing { since: at(2) });
}

#[test]
fn speech_that_resumes_stays_accepted() {
    // Speech, a pause, a short word (one frame), a pause: still one utterance, never waiting.
    let states = run(&[S, S, S, S, N, N, S, N]);
    assert_eq!(states[4], Endpoint::Trailing { since: at(4) });
    assert!(matches!(states[6], Endpoint::InSpeech { .. }));
    assert_eq!(states[7], Endpoint::Trailing { since: at(7) });
}

#[test]
fn ended_is_final() {
    for why in [EndWhy::Silence, EndWhy::NoSpeech] {
        for v in [S, N] {
            assert_eq!(
                endpoint(Endpoint::Ended(why), v, SampleIndex(1 << 40), &params()),
                Endpoint::Ended(why)
            );
        }
    }
}

proptest! {
    #[test]
    fn endpoint_is_total_and_ended_sticks(verdicts in proptest::collection::vec(any::<bool>(), 0..200)) {
        let verdicts: Vec<Voiced> = verdicts.into_iter().map(|b| if b { S } else { N }).collect();
        let states = run(&verdicts);
        let mut ended = false;
        for (i, state) in states.iter().enumerate() {
            if ended {
                prop_assert!(matches!(state, Endpoint::Ended(_)));
            }
            ended |= matches!(state, Endpoint::Ended(_));
            if let Endpoint::InSpeech { since } = state {
                prop_assert!(since.0 <= i as u64 * 512);
            }
        }
    }
}

#[test]
fn the_default_params_are_the_proposal() {
    let p = EndpointParams::default();
    assert_eq!(
        (p.lead, p.silence_end, p.min_speech),
        (AudioMs(300), AudioMs(30_000), AudioMs(150))
    );
}

/// Framer, gate and endpointer over a 3 s recording: 0.5 s of quiet, 1 s of tone, then quiet.
#[test]
fn the_chain_finds_the_utterance_in_synthetic_audio() {
    let rate = 16_000_usize;
    let mut samples = vec![0_i16; rate / 2];
    samples.extend((0..rate).map(|i| if (i / 40) % 2 == 0 { 6000 } else { -6000 }));
    samples.extend(vec![0_i16; rate * 2]);
    let params = EndpointParams {
        lead: AudioMs(300),
        silence_end: AudioMs(1000),
        min_speech: AudioMs(150),
    };
    let mut framer = Framer::new();
    let mut gate = EnergyGate::new(EnergyGateParams::default());
    let mut state = Endpoint::Waiting;
    let mut started = None;
    // Feed it in odd-sized chunks, as a microphone would.
    for (n, piece) in bytes(&samples).chunks(3001).enumerate() {
        let at = (n * 3001 / 2) as u64;
        for framed in framer.push(&chunk(at, piece.to_vec())).unwrap() {
            let (voiced, _) = gate.push(&framed.frame);
            state = endpoint(state, voiced, framed.at, &params);
            if let (Endpoint::InSpeech { since }, None) = (state, started) {
                started = Some(since);
            }
        }
    }
    // Speech starts at sample 8000 (frame 15 starts at 7680, frame 16 at 8192: the first frame
    // wholly loud starts at 8192, and the one before it straddles the start).
    let since = started.expect("speech was found");
    assert!((7680..=8192).contains(&since.0), "{since:?}");
    assert_eq!(state, Endpoint::Ended(EndWhy::Silence));
}
