use std::path::PathBuf;

use cua_action::{CuaDialect, GridMax, ModelSpace, ToolDialect};
use model_catalog::{
    CatalogError, CatalogId, CatalogKind, EngineKind, MiB, VramEstimate, merge_catalogs,
    parse_entry,
};
use model_provider::{CuaSupport, Tokens};
use vision_prep::{PatchFactor, PixelCount, ResizeRule};

fn catalog_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog")
}

fn holo_text() -> String {
    std::fs::read_to_string(catalog_dir().join("holo-3.1-4b.toml")).unwrap()
}

#[test]
fn shipped_files_parse() {
    let mut parsed = 0;
    for file in std::fs::read_dir(catalog_dir()).unwrap() {
        let path = file.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        let entry = parse_entry(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            path.file_stem().unwrap().to_str().unwrap(),
            entry.id.0,
            "the id is the file's stem"
        );
        parsed += 1;
    }
    assert_eq!(parsed, 1);
}

#[test]
fn holo_entry_says_what_s0_found() {
    let entry = parse_entry(&holo_text()).unwrap();
    assert_eq!(
        entry.caps.images.rule,
        ResizeRule::SmartResize {
            factor: PatchFactor(32),
            min_pixels: PixelCount(65_536),
            max_pixels: PixelCount(16_777_216),
        }
    );
    assert_eq!(entry.caps.images.space, ModelSpace::Grid(GridMax(1000)));
    assert!(matches!(
        entry.caps.computer_use,
        CuaSupport::Dialect {
            dialect: CuaDialect::Tool(ToolDialect::Holo31),
            ..
        }
    ));
    assert_eq!(
        entry.roles,
        [CatalogKind::Llm, CatalogKind::ComputerUse].into()
    );
    assert_eq!(entry.engines.len(), 1);
    assert_eq!(entry.engines[0].kind, EngineKind::Vllm);
}

#[test]
fn entry_round_trips_through_toml() {
    let entry = parse_entry(&holo_text()).unwrap();
    let text = toml::to_string(&entry).unwrap();
    assert_eq!(parse_entry(&text).unwrap(), entry);
}

#[test]
fn missing_field_refuses() {
    let text = holo_text();
    for field in [
        "label",
        "licence",
        "vram",
        "context",
        "images",
        "computer_use",
        "roles",
        "tools",
    ] {
        let without: String = text
            .lines()
            .filter(|l| !l.starts_with(&format!("{field} ")))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            matches!(parse_entry(&without), Err(CatalogError::Toml(_))),
            "a file without {field} must be refused"
        );
    }
}

#[test]
fn an_entry_without_an_engine_is_refused() {
    let text = holo_text();
    let head = text.split("[[engine]]").next().unwrap();
    // With no `[[engine]]` table the field is missing, which serde reports before the check.
    assert!(matches!(parse_entry(head), Err(CatalogError::Toml(_))));
}

#[test]
fn user_file_replaces_system() {
    let system = parse_entry(&holo_text()).unwrap();
    let mut user = system.clone();
    user.label = "My Holo".into();
    let mut extra = system.clone();
    extra.id = CatalogId("mine".into());
    let merged = merge_catalogs(vec![system.clone()], vec![user.clone(), extra.clone()]);
    assert_eq!(merged, vec![user, extra]);
}

#[test]
fn vram_need_adds_weights_kv_and_overhead() {
    let vram = VramEstimate {
        weights: MiB(10_400),
        kv_per_1k_ctx: MiB(32),
        overhead: MiB(1500),
    };
    assert_eq!(vram.need(Tokens(32_768)), MiB(10_400 + 1049 + 1500)); // 32 * 32.768 = 1048.6 rounds up
    assert_eq!(vram.need(Tokens(0)), MiB(11_900));
}
