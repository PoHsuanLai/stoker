//! `command`: the unit each engine kind gets, from the shipped catalog entries.

use std::path::PathBuf;

use engine_supervisor::{
    EnginePaths, EnvPair, GpuAccess, Network, ProgramPath, Sandbox, SocketPath, UnitSpec, command,
};
use model_catalog::{
    EngineArg, EngineKind, EngineProfile, FileName, MiB, ModelEntry, WeightFiles, parse_entry,
};

const HOLO: &str = include_str!("../../../catalog/holo-3.1-4b.toml");
const KOKORO: &str = include_str!("../../../catalog/kokoro-82m.toml");
const NEMOTRON: &str = include_str!("../../../catalog/nemotron-3.5-asr-streaming.toml");

fn paths() -> EnginePaths {
    EnginePaths {
        vllm_python: ProgramPath(PathBuf::from("/opt/vllm/bin/python")),
        llama_server: ProgramPath(PathBuf::from("/opt/llama/bin/llama-server")),
        speech_host: ProgramPath(PathBuf::from("/opt/stoker/bin/speech-host")),
        kokoro_python: ProgramPath(PathBuf::from("/opt/kokoro/bin/python")),
        hf_cache: PathBuf::from("/data/hf/hub"),
    }
}

fn socket(name: &str) -> SocketPath {
    SocketPath(PathBuf::from(format!("/run/user/1000/inferd/{name}.sock")))
}

fn args(unit: &UnitSpec) -> Vec<&str> {
    unit.args.iter().map(|a| a.0.as_str()).collect()
}

fn entry(text: &str) -> ModelEntry {
    parse_entry(text).unwrap()
}

const HOLO_DIR: &str =
    "/data/hf/hub/models--Hcompany--Holo-3.1-4B/snapshots/8c88265a5a159bfd1492db9243733dd2e6e04a6e";

#[test]
fn holo_under_vllm_is_the_pinned_unit() {
    let holo = entry(HOLO);
    let unit = command(&holo, &holo.engines[0], &paths(), &socket("vllm-holo"));
    assert_eq!(
        unit,
        UnitSpec {
            program: ProgramPath(PathBuf::from("/opt/vllm/bin/python")),
            args: [
                "-m",
                "vllm.entrypoints.openai.api_server",
                "--model",
                HOLO_DIR,
                "--served-model-name",
                "holo-3.1-4b",
                "--uds",
                "/run/user/1000/inferd/vllm-holo.sock",
                "--max-model-len",
                "32768",
                "--limit-mm-per-prompt",
                "{\"image\":3,\"video\":0}",
                "--enable-auto-tool-choice",
                "--tool-call-parser",
                "qwen3_coder",
                "--reasoning-parser",
                "qwen3",
                "--gpu-memory-utilization",
                "0.80",
            ]
            .map(|a| EngineArg(a.into()))
            .to_vec(),
            env: vec![EnvPair {
                name: "HF_HUB_OFFLINE".into(),
                value: "1".into()
            }],
            sandbox: Sandbox {
                network: Network::None,
                read: vec![PathBuf::from("/data/hf/hub/models--Hcompany--Holo-3.1-4B")],
                write: vec![PathBuf::from("/run/user/1000/inferd")],
                gpu: GpuAccess::Nvidia,
                memory_max: MiB(4096 + 10400 + 1500),
            },
        }
    );
}

#[test]
fn llama_server_gets_the_gguf_files_and_the_socket_as_its_host() {
    let holo = entry(HOLO);
    let profile = EngineProfile {
        kind: EngineKind::LlamaServer,
        args: vec![
            EngineArg("--jinja".into()),
            EngineArg("--ctx-size".into()),
            EngineArg("32768".into()),
            EngineArg("--special".into()),
        ],
        weights: WeightFiles::Gguf {
            model: FileName("holo-q4_k_m.gguf".into()),
            mmproj: FileName("mmproj-f16.gguf".into()),
        },
    };
    let unit = command(&holo, &profile, &paths(), &socket("llama-holo"));
    assert_eq!(
        unit.program,
        ProgramPath(PathBuf::from("/opt/llama/bin/llama-server"))
    );
    assert_eq!(
        args(&unit),
        [
            "--model",
            &format!("{HOLO_DIR}/holo-q4_k_m.gguf"),
            "--mmproj",
            &format!("{HOLO_DIR}/mmproj-f16.gguf"),
            "--alias",
            "holo-3.1-4b",
            "--host",
            "/run/user/1000/inferd/llama-holo.sock",
            "--jinja",
            "--ctx-size",
            "32768",
            "--special",
        ]
    );
    assert_eq!(unit.sandbox.gpu, GpuAccess::Nvidia);
}

#[test]
fn the_speech_host_runs_on_the_cpu_with_the_weights_directory_and_the_socket() {
    let nemotron = entry(NEMOTRON);
    let unit = command(
        &nemotron,
        &nemotron.engines[0],
        &paths(),
        &socket("speech-in"),
    );
    assert_eq!(
        unit.program,
        ProgramPath(PathBuf::from("/opt/stoker/bin/speech-host"))
    );
    assert_eq!(
        args(&unit),
        [
            "--model-dir",
            "/data/hf/hub/models--csukuangfj2--sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11/snapshots/ab43d895f5985b1bbab8b6eac8607fcdc05343f3",
            "--socket",
            "/run/user/1000/inferd/speech-in.sock",
            "--threads",
            "6",
            "--chunk-ms",
            "560",
        ]
    );
    assert_eq!(
        unit.sandbox.gpu,
        GpuAccess::Absent,
        "an all-zero estimate means no GPU"
    );
    assert_eq!(
        unit.sandbox.memory_max,
        MiB(4096),
        "a CPU entry still has a host-memory cap"
    );
}

#[test]
fn kokoro_runs_uvicorn_in_its_own_environment_on_the_socket() {
    let kokoro = entry(KOKORO);
    let unit = command(&kokoro, &kokoro.engines[0], &paths(), &socket("kokoro"));
    assert_eq!(
        unit.program,
        ProgramPath(PathBuf::from("/opt/kokoro/bin/python"))
    );
    assert_eq!(
        args(&unit),
        [
            "-m",
            "uvicorn",
            "api.src.main:app",
            "--uds",
            "/run/user/1000/inferd/kokoro.sock",
        ]
    );
    assert_eq!(unit.sandbox.gpu, GpuAccess::Absent);
}

#[test]
fn the_socket_placeholder_is_replaced_wherever_it_appears() {
    let holo = entry(HOLO);
    let profile = EngineProfile {
        kind: EngineKind::SpeechHost,
        args: vec![
            EngineArg("--bind=unix:{socket}".into()),
            EngineArg("{socket}:{socket}".into()),
            EngineArg("--plain".into()),
        ],
        weights: WeightFiles::SherpaDir,
    };
    let unit = command(&holo, &profile, &paths(), &socket("x"));
    let s = "/run/user/1000/inferd/x.sock";
    assert!(args(&unit).contains(&format!("--bind=unix:{s}").as_str()));
    assert!(args(&unit).contains(&format!("{s}:{s}").as_str()));
    assert!(args(&unit).iter().all(|a| !a.contains("{socket}")));
}

#[test]
fn every_engine_starts_offline_with_no_network_and_a_writable_socket_directory_only() {
    for (text, which) in [(HOLO, 0), (KOKORO, 0), (NEMOTRON, 0)] {
        let e = entry(text);
        let unit = command(&e, &e.engines[which], &paths(), &socket("any"));
        assert_eq!(unit.env.len(), 1);
        assert_eq!(
            (unit.env[0].name.as_str(), unit.env[0].value.as_str()),
            ("HF_HUB_OFFLINE", "1")
        );
        assert_eq!(unit.sandbox.network, Network::None);
        assert_eq!(
            unit.sandbox.write,
            vec![PathBuf::from("/run/user/1000/inferd")]
        );
        assert_eq!(
            unit.sandbox.read.len(),
            1,
            "the weights directory, read-only"
        );
    }
}

/// No component walks up or is a root: joined to the cache, the path stays under it.
fn stays_inside(path: &std::path::Path) -> bool {
    path.starts_with("/data/hf/hub")
        && path
            .components()
            .all(|c| !matches!(c, std::path::Component::ParentDir))
}

#[test]
fn a_repository_or_revision_cannot_leave_the_cache() {
    let mut holo = entry(HOLO);
    let model_catalog::WeightSource::HuggingFace { repo, revision } = &mut holo.source;
    repo.0 = "../../etc/passwd".into();
    revision.0 = "../../../root".into();
    let unit = command(&holo, &holo.engines[0], &paths(), &socket("x"));
    let model_dir = unit.args[3].0.clone();
    assert!(
        model_dir.starts_with("/data/hf/hub/models--"),
        "{model_dir}"
    );
    assert!(stays_inside(&PathBuf::from(&model_dir)), "{model_dir}");
    assert!(unit.sandbox.read[0].starts_with("/data/hf/hub"));
    assert!(stays_inside(&unit.sandbox.read[0]));
}

#[test]
fn a_gguf_file_name_with_a_directory_part_stays_in_the_snapshot() {
    let holo = entry(HOLO);
    let profile = EngineProfile {
        kind: EngineKind::LlamaServer,
        args: vec![],
        weights: WeightFiles::Gguf {
            model: FileName("../../outside/model.gguf".into()),
            mmproj: FileName("/abs/mmproj.gguf".into()),
        },
    };
    let unit = command(&holo, &profile, &paths(), &socket("x"));
    assert_eq!(unit.args[1].0, format!("{HOLO_DIR}/model.gguf"));
    assert_eq!(unit.args[3].0, format!("{HOLO_DIR}/mmproj.gguf"));
}

#[test]
fn llama_server_with_weights_that_are_not_gguf_gets_no_model_and_no_panic() {
    let holo = entry(HOLO);
    let profile = EngineProfile {
        kind: EngineKind::LlamaServer,
        args: vec![],
        weights: WeightFiles::HfSnapshot,
    };
    let unit = command(&holo, &profile, &paths(), &socket("x"));
    assert!(!args(&unit).contains(&"--model"));
}

#[test]
fn command_is_pure_and_total_over_arbitrary_text() {
    use proptest::prelude::*;
    let holo = entry(HOLO);
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(
            &(".{0,40}", ".{0,40}", ".{0,40}", ".{0,40}"),
            |(repo, revision, arg, file)| {
                let mut e = holo.clone();
                let model_catalog::WeightSource::HuggingFace {
                    repo: r,
                    revision: v,
                } = &mut e.source;
                r.0 = repo;
                v.0 = revision;
                let profile = EngineProfile {
                    kind: EngineKind::LlamaServer,
                    args: vec![EngineArg(arg)],
                    weights: WeightFiles::Gguf {
                        model: FileName(file.clone()),
                        mmproj: FileName(file),
                    },
                };
                let a = command(&e, &profile, &paths(), &socket("p"));
                let b = command(&e, &profile, &paths(), &socket("p"));
                prop_assert_eq!(&a, &b);
                for path in &a.sandbox.read {
                    prop_assert!(stays_inside(path));
                }
                for arg in a.args.iter().filter(|a| a.0.starts_with("/data/hf/hub")) {
                    prop_assert!(stays_inside(std::path::Path::new(&arg.0)), "{}", arg.0);
                }
                Ok(())
            },
        )
        .unwrap();
}
