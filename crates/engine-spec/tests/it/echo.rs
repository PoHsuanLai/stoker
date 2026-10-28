//! A new engine needs only its impl: the trait's defaults cover the rest.

use std::path::Path;

use engine_spec::{Engine, Launch, LaunchArgs};
use model_catalog::{EngineArg, WeightFiles, parse_entry};

#[derive(Debug)]
struct Echo;

impl Engine for Echo {
    fn slug(&self) -> &'static str {
        "echo"
    }
}

#[test]
fn an_engine_that_writes_only_its_slug_gets_the_defaults() {
    let echo: &dyn Engine = &Echo;
    assert_eq!(echo.disk_word(), "echo");
    assert!(echo.packages().is_empty());
    assert!(echo.package_paths().is_empty());
    assert!(echo.pointed().is_none());
    assert!(echo.accepts(&WeightFiles::SherpaDir));
    assert!(!echo.requires_gpu_estimate());
    assert_eq!(echo.bus_word(), "echo");
    assert_eq!(echo.running_word(), "echo");
    assert!(echo.seat_flags().is_none());
    assert!(!echo.caches_prompts() && !echo.hears() && !echo.speaks());
    assert_eq!(echo.env(None), engine_spec::HostEnv::default());
    let entry = parse_entry(include_str!("../../../../catalog/holo-3.1-4b.toml")).unwrap();
    let launch = Launch {
        entry: &entry,
        weights: &entry.engines[0].weights,
        snapshot: Path::new("/cache/snap"),
        socket: "/run/echo.sock",
    };
    assert_eq!(echo.command(&launch), LaunchArgs::default());
}

#[test]
fn an_engine_overrides_only_what_differs() {
    #[derive(Debug)]
    struct Loud;
    impl Engine for Loud {
        fn slug(&self) -> &'static str {
            "loud"
        }
        fn command(&self, launch: &Launch<'_>) -> LaunchArgs {
            LaunchArgs {
                front: vec![EngineArg("--sock".into())],
                back: vec![EngineArg(launch.socket.to_owned())],
            }
        }
    }
    let entry = parse_entry(include_str!("../../../../catalog/holo-3.1-4b.toml")).unwrap();
    let launch = Launch {
        entry: &entry,
        weights: &entry.engines[0].weights,
        snapshot: Path::new("/cache/snap"),
        socket: "/run/loud.sock",
    };
    let args = Loud.command(&launch);
    assert_eq!(args.front, [EngineArg("--sock".into())]);
    assert_eq!(args.back, [EngineArg("/run/loud.sock".into())]);
    assert_eq!(Loud.disk_word(), "loud");
}
