use std::path::PathBuf;
use std::time::Duration;

use engine_supervisor::{
    Attempt, BudgetVerdict, EngineFailure, EngineId, EngineSpec, EngineState, EnvPair, ExitCode,
    GpuAccess, GpuMemory, MonoMs, Network, Probe, ProgramPath, Sandbox, Supervisor,
    SupervisorConfig, SupervisorIn, UnitSpec,
};
use model_catalog::{EngineArg, MiB};

fn round_trip<T>(value: &T, json: &str)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + core::fmt::Debug,
{
    assert_eq!(serde_json::to_string(value).unwrap(), json);
    assert_eq!(&serde_json::from_str::<T>(json).unwrap(), value);
}

fn unit() -> UnitSpec {
    UnitSpec {
        program: ProgramPath(PathBuf::from("/opt/vllm/bin/python")),
        args: vec![EngineArg("-m".into()), EngineArg("vllm".into())],
        env: vec![EnvPair {
            name: "HF_HUB_OFFLINE".into(),
            value: "1".into(),
        }],
        sandbox: Sandbox {
            network: Network::None,
            read: vec![PathBuf::from("/home/u/.cache/huggingface")],
            write: vec![],
            gpu: GpuAccess::Nvidia,
            memory_max: MiB(16_384),
        },
    }
}

#[test]
fn states_round_trip_with_pinned_json() {
    round_trip(&EngineState::Stopped, r#"{"kind":"stopped"}"#);
    round_trip(
        &EngineState::Starting {
            since: MonoMs(10),
            attempt: Attempt(1),
        },
        r#"{"kind":"starting","v":{"since":10,"attempt":1}}"#,
    );
    round_trip(
        &EngineState::Ready {
            since: MonoMs(20),
            last_used: MonoMs(30),
        },
        r#"{"kind":"ready","v":{"since":20,"last_used":30}}"#,
    );
    round_trip(
        &EngineState::Failed(EngineFailure::NoRoom {
            need: MiB(12_000),
            free: MiB(4_000),
        }),
        r#"{"kind":"failed","v":{"kind":"no_room","v":{"need":12000,"free":4000}}}"#,
    );
    round_trip(
        &EngineState::Failed(EngineFailure::Exited {
            code: ExitCode(137),
        }),
        r#"{"kind":"failed","v":{"kind":"exited","v":{"code":137}}}"#,
    );
}

#[test]
fn inputs_and_verdicts_round_trip() {
    round_trip(
        &SupervisorIn::Want(EngineId("vllm:holo-3.1-4b".into())),
        r#"{"kind":"want","v":"vllm:holo-3.1-4b"}"#,
    );
    round_trip(
        &SupervisorIn::Probed {
            id: EngineId("e".into()),
            probe: Probe::Loading,
        },
        r#"{"kind":"probed","v":{"id":"e","probe":"loading"}}"#,
    );
    round_trip(&SupervisorIn::Tick(MonoMs(5)), r#"{"kind":"tick","v":5}"#);
    round_trip(
        &SupervisorIn::Gpu(GpuMemory {
            total: MiB(16_303),
            used_by_others: MiB(2_000),
        }),
        r#"{"kind":"gpu","v":{"total":16303,"used_by_others":2000}}"#,
    );
    round_trip(&BudgetVerdict::Fits, r#"{"kind":"fits"}"#);
    round_trip(
        &BudgetVerdict::EvictFirst(vec![EngineId("a".into())]),
        r#"{"kind":"evict_first","v":["a"]}"#,
    );
}

#[test]
fn proposed_config_values() {
    let cfg = SupervisorConfig::default();
    assert_eq!(cfg.idle_unload, Duration::from_secs(600));
    assert_eq!(cfg.start_timeout, Duration::from_secs(180));
    assert_eq!(cfg.probe_every, Duration::from_millis(500));
    assert_eq!(cfg.max_attempts, Attempt(3));
    assert_eq!(
        cfg.backoff,
        (Duration::from_secs(2), Duration::from_secs(30))
    );
    assert_eq!(cfg.headroom, MiB(1024));
}

#[test]
fn a_new_supervisor_has_every_engine_stopped() {
    let spec = |name: &str| EngineSpec {
        id: EngineId(name.into()),
        need: MiB(11_900),
        unit: unit(),
    };
    let gpu = GpuMemory {
        total: MiB(16_303),
        used_by_others: MiB(0),
    };
    let s = Supervisor::new(vec![spec("a"), spec("b")], gpu);
    assert_eq!(s.state(&EngineId("a".into())), Some(&EngineState::Stopped));
    assert_eq!(s.state(&EngineId("b".into())), Some(&EngineState::Stopped));
    assert_eq!(s.state(&EngineId("c".into())), None);
    assert_eq!(s.gpu(), gpu);
}
