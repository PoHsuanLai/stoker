//! One table per row of models §4.1, plus the crash cap.

use super::*;
use crate::testutil::{id, spec};
use SupervisorIn::{Exited, Gpu, Probed, Tick, Used, Want};
use SupervisorOut::{Changed, Probe as ProbeOut, Spawn, Stop, WakeAt};

const GPU: GpuMemory = GpuMemory {
    total: MiB(16000),
    used_by_others: MiB(0),
};

fn cfg() -> SupervisorConfig {
    SupervisorConfig {
        headroom: MiB(0),
        ..SupervisorConfig::default()
    }
}

fn sup() -> Supervisor {
    Supervisor::new(vec![spec("a", 9000), spec("b", 9000), spec("c", 2000)], GPU)
}

fn run(mut s: Supervisor, inputs: Vec<SupervisorIn>) -> (Supervisor, Vec<SupervisorOut>) {
    let mut all = Vec::new();
    for input in inputs {
        let (next, out) = step(s, input, &cfg());
        s = next;
        all.extend(out);
    }
    (s, all)
}

fn state(s: &Supervisor, name: &str) -> EngineState {
    *s.state(&id(name)).expect("known engine")
}

fn unit(name: &str) -> UnitSpec {
    let need = if name == "c" { 2000 } else { 9000 };
    spec(name, need).unit
}

fn up(s: Supervisor, name: &str, at: u64) -> Supervisor {
    run(
        s,
        vec![
            Tick(MonoMs(at)),
            Want(id(name)),
            Probed {
                id: id(name),
                probe: Probe::Ready,
            },
        ],
    )
    .0
}

#[test]
fn want_when_it_fits_starts() {
    let (s, out) = run(sup(), vec![Tick(MonoMs(1000)), Want(id("a"))]);
    let starting = EngineState::Starting {
        since: MonoMs(1000),
        attempt: Attempt(1),
    };
    assert_eq!(state(&s, "a"), starting);
    assert_eq!(
        out,
        vec![
            Spawn(id("a"), unit("a")),
            ProbeOut(id("a")),
            WakeAt(MonoMs(1500)),
            Changed(id("a"), starting),
        ]
    );
}

#[test]
fn want_with_no_room_fails_with_numbers() {
    let gpu = GpuMemory {
        total: MiB(8000),
        used_by_others: MiB(500),
    };
    let (s, out) = run(
        Supervisor::new(vec![spec("a", 9000)], gpu),
        vec![Want(id("a"))],
    );
    let failed = EngineState::Failed(EngineFailure::NoRoom {
        need: MiB(9000),
        free: MiB(7500),
    });
    assert_eq!(state(&s, "a"), failed);
    assert_eq!(out, vec![Changed(id("a"), failed)]);
}

#[test]
fn want_evicts_then_starts_when_the_victim_exits() {
    let s = up(sup(), "a", 0);
    let s = run(s, vec![Tick(MonoMs(5000))]).0;
    let (s, out) = run(s, vec![Want(id("b"))]);
    let stopping = EngineState::Stopping {
        since: MonoMs(5000),
    };
    assert_eq!(state(&s, "a"), stopping);
    assert_eq!(state(&s, "b"), EngineState::Stopped);
    assert_eq!(out, vec![Stop(id("a")), Changed(id("a"), stopping)]);
    let (s, out) = run(
        s,
        vec![Exited {
            id: id("a"),
            code: ExitCode(0),
        }],
    );
    let starting = EngineState::Starting {
        since: MonoMs(5000),
        attempt: Attempt(1),
    };
    assert_eq!(state(&s, "a"), EngineState::Stopped);
    assert_eq!(state(&s, "b"), starting);
    assert_eq!(
        out,
        vec![
            Changed(id("a"), EngineState::Stopped),
            Spawn(id("b"), unit("b")),
            ProbeOut(id("b")),
            WakeAt(MonoMs(5500)),
            Changed(id("b"), starting),
        ]
    );
}

#[test]
fn an_engine_in_a_turn_is_never_evicted() {
    let s = up(sup(), "a", 0);
    let (s, out) = run(s, vec![Tick(MonoMs(100)), Used(id("a")), Want(id("b"))]);
    assert!(matches!(state(&s, "a"), EngineState::Ready { .. }));
    assert!(matches!(
        state(&s, "b"),
        EngineState::Failed(EngineFailure::NoRoom { .. })
    ));
    assert!(!out.contains(&Stop(id("a"))));
}

#[test]
fn an_idle_engine_is_evicted_once_the_probe_interval_passes() {
    let s = up(sup(), "a", 0);
    let (s, _) = run(
        s,
        vec![
            Tick(MonoMs(1000)),
            Used(id("a")),
            Tick(MonoMs(1600)),
            Want(id("b")),
        ],
    );
    assert!(matches!(state(&s, "a"), EngineState::Stopping { .. }));
}

#[test]
fn starting_probe_ready_makes_ready() {
    let s = run(sup(), vec![Tick(MonoMs(0)), Want(id("c"))]).0;
    let (s, out) = run(
        s,
        vec![
            Tick(MonoMs(700)),
            Probed {
                id: id("c"),
                probe: Probe::Ready,
            },
        ],
    );
    let ready = EngineState::Ready {
        since: MonoMs(700),
        last_used: MonoMs(700),
    };
    assert_eq!(state(&s, "c"), ready);
    assert_eq!(out, vec![WakeAt(MonoMs(600_700)), Changed(id("c"), ready)]);
}

#[test]
fn starting_probe_loading_or_down_probes_again() {
    for probe in [Probe::Loading, Probe::Down] {
        let s = run(sup(), vec![Tick(MonoMs(0)), Want(id("c"))]).0;
        let (s, out) = run(s, vec![Tick(MonoMs(500)), Probed { id: id("c"), probe }]);
        assert!(
            matches!(state(&s, "c"), EngineState::Starting { .. }),
            "{probe:?}"
        );
        assert_eq!(
            out,
            vec![WakeAt(MonoMs(1000)), ProbeOut(id("c"))],
            "{probe:?}"
        );
    }
}

#[test]
fn start_timeout_backs_off_then_fails_never_ready() {
    let mut s = run(sup(), vec![Tick(MonoMs(0)), Want(id("c"))]).0;
    let timeout = 180_000;
    let (next, out) = run(s, vec![Tick(MonoMs(timeout))]);
    s = next;
    let backoff = EngineState::Backoff {
        until: MonoMs(timeout + 2000),
        attempt: Attempt(2),
    };
    assert_eq!(state(&s, "c"), backoff);
    assert_eq!(
        out,
        vec![
            Stop(id("c")),
            WakeAt(MonoMs(timeout + 2000)),
            Changed(id("c"), backoff)
        ]
    );
    let (s, _) = run(s, vec![Tick(MonoMs(timeout + 2000))]);
    assert_eq!(
        state(&s, "c"),
        EngineState::Starting {
            since: MonoMs(timeout + 2000),
            attempt: Attempt(2)
        }
    );
    let (s, _) = run(s, vec![Tick(MonoMs(2 * timeout + 2000))]);
    let until = 2 * timeout + 2000 + 4000;
    assert_eq!(
        state(&s, "c"),
        EngineState::Backoff {
            until: MonoMs(until),
            attempt: Attempt(3)
        }
    );
    let (s, _) = run(s, vec![Tick(MonoMs(until)), Tick(MonoMs(until + timeout))]);
    assert_eq!(
        state(&s, "c"),
        EngineState::Failed(EngineFailure::NeverReady)
    );
}

#[test]
fn crash_backoff_caps_attempts() {
    let code = ExitCode(137);
    let mut s = run(sup(), vec![Tick(MonoMs(0)), Want(id("c"))]).0;
    let waits = [2000u64, 4000];
    let mut now = 0u64;
    for (n, wait) in waits.iter().enumerate() {
        let (next, out) = run(s, vec![Exited { id: id("c"), code }]);
        let attempt = Attempt(u8::try_from(n + 2).expect("small"));
        let backoff = EngineState::Backoff {
            until: MonoMs(now + wait),
            attempt,
        };
        assert_eq!(state(&next, "c"), backoff);
        assert_eq!(
            out,
            vec![WakeAt(MonoMs(now + wait)), Changed(id("c"), backoff)]
        );
        now += wait;
        let (again, out) = run(next, vec![Tick(MonoMs(now))]);
        assert!(out.contains(&Spawn(id("c"), unit("c"))));
        s = again;
    }
    let (s, out) = run(s, vec![Exited { id: id("c"), code }]);
    let failed = EngineState::Failed(EngineFailure::Exited { code });
    assert_eq!(state(&s, "c"), failed);
    assert_eq!(out, vec![Changed(id("c"), failed)]);
}

#[test]
fn backoff_doubles_and_caps() {
    let c = cfg();
    let table = [
        (1u8, 2u64),
        (2, 4),
        (3, 8),
        (4, 16),
        (5, 30),
        (40, 30),
        (255, 30),
    ];
    for (attempt, secs) in table {
        assert_eq!(
            backoff(Attempt(attempt), &c),
            Duration::from_secs(secs),
            "{attempt}"
        );
    }
}

#[test]
fn ready_exit_backs_off() {
    let s = up(sup(), "c", 0);
    let (s, _) = run(s, vec![Tick(MonoMs(10_000))]);
    let (s, out) = run(
        s,
        vec![Exited {
            id: id("c"),
            code: ExitCode(1),
        }],
    );
    let backoff = EngineState::Backoff {
        until: MonoMs(12_000),
        attempt: Attempt(2),
    };
    assert_eq!(state(&s, "c"), backoff);
    assert_eq!(out, vec![WakeAt(MonoMs(12_000)), Changed(id("c"), backoff)]);
}

#[test]
fn used_refreshes_last_used_with_no_effects() {
    let s = up(sup(), "c", 0);
    let (s, out) = run(s, vec![Tick(MonoMs(9000)), Used(id("c"))]);
    assert_eq!(
        state(&s, "c"),
        EngineState::Ready {
            since: MonoMs(0),
            last_used: MonoMs(9000)
        }
    );
    assert!(out.len() == 1, "only the tick's WakeAt: {out:?}");
}

#[test]
fn idle_unload_stops_then_exit_stops() {
    let s = up(sup(), "c", 0);
    let (s, out) = run(s, vec![Tick(MonoMs(599_999))]);
    assert_eq!(out, vec![WakeAt(MonoMs(600_000))]);
    let (s, out) = run(s, vec![Tick(MonoMs(600_000))]);
    let stopping = EngineState::Stopping {
        since: MonoMs(600_000),
    };
    assert_eq!(out, vec![Stop(id("c")), Changed(id("c"), stopping)]);
    let (s, out) = run(
        s,
        vec![Exited {
            id: id("c"),
            code: ExitCode(0),
        }],
    );
    assert_eq!(state(&s, "c"), EngineState::Stopped);
    assert_eq!(out, vec![Changed(id("c"), EngineState::Stopped)]);
}

#[test]
fn failed_want_resets_the_attempt() {
    let gpu = GpuMemory {
        total: MiB(1000),
        used_by_others: MiB(0),
    };
    let s = Supervisor::new(vec![spec("a", 9000)], gpu);
    let (s, _) = run(s, vec![Want(id("a"))]);
    assert!(matches!(state(&s, "a"), EngineState::Failed(_)));
    let (s, _) = run(s, vec![Gpu(GPU), Want(id("a"))]);
    assert_eq!(s.gpu(), GPU);
    assert_eq!(
        state(&s, "a"),
        EngineState::Starting {
            since: MonoMs(0),
            attempt: Attempt(1)
        }
    );
}

#[test]
fn gpu_input_is_stored_and_unknown_ids_are_ignored() {
    let other = GpuMemory {
        total: MiB(1),
        used_by_others: MiB(1),
    };
    let (s, out) = run(sup(), vec![Gpu(other), Want(id("nope")), Used(id("nope"))]);
    assert_eq!(s.gpu(), other);
    assert!(out.is_empty());
}

#[test]
fn stale_inputs_change_nothing() {
    let before = sup();
    let (s, out) = run(
        before.clone(),
        vec![
            Probed {
                id: id("a"),
                probe: Probe::Ready,
            },
            Exited {
                id: id("a"),
                code: ExitCode(0),
            },
            Used(id("a")),
        ],
    );
    assert_eq!(s, before);
    assert!(out.is_empty());
}

#[test]
fn headroom_counts_against_the_fit() {
    let tight = SupervisorConfig {
        headroom: MiB(1024),
        ..SupervisorConfig::default()
    };
    let s = Supervisor::new(vec![spec("a", 15_500)], GPU);
    let (s, _) = step(s, Want(id("a")), &tight);
    assert!(matches!(
        state(&s, "a"),
        EngineState::Failed(EngineFailure::NoRoom { .. })
    ));
}
