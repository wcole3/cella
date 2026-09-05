use cella_lib::config::{CellaConfig, Config1D, Config2D};
use cella_lib::{Rule1D, Rule2D};
use std::fs;
use std::path::PathBuf;

fn temp_file_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("cella_test_{}_{}", std::process::id(), name));
    path
}

#[test]
fn test_config_1d_file_io() {
    let path = temp_file_path("1d.json");
    let init = vec!["A".to_string(), "B".to_string()];
    let rule = Rule1D { subrules: vec![] };
    let cfg = CellaConfig::D1(Config1D {
        colors: Default::default(),
        seed: 0,
        ensemble: None,
        evolve: None,
        width: 2,
        history_limit: 5,
        initial: init.clone(),
        rule: rule.clone(),
    });

    // Save
    cfg.to_file_pretty(&path).expect("Failed to save config");

    // Load
    let loaded = CellaConfig::from_file(&path).expect("Failed to load config");

    // Cleanup
    let _ = fs::remove_file(&path);

    match loaded {
        CellaConfig::D1(c) => {
            assert_eq!(c.width, 2);
            assert_eq!(c.history_limit, 5);
            assert_eq!(c.initial, init);
            // Rule comparison might be tricky if not Eq, but empty rule is easy
            assert!(c.rule.subrules.is_empty());
        }
        _ => panic!("Expected D1 config"),
    }
}

#[test]
fn test_config_2d_file_io() {
    let path = temp_file_path("2d.json");
    let init = vec![
        "A".to_string(),
        "B".to_string(),
        "C".to_string(),
        "D".to_string(),
    ];
    let rule = Rule2D { subrules: vec![] };
    let cfg = CellaConfig::D2(Config2D {
        ensemble: None,
        evolve: None,
        seed: 0,
        colors: Default::default(),
        width: 2,
        height: 2,
        history_limit: 3,
        initial: init.clone(),
        rule: rule.clone(),
        model: None,
    });

    // Save
    cfg.to_file_pretty(&path).expect("Failed to save config");

    // Load
    let loaded = CellaConfig::from_file(&path).expect("Failed to load config");

    // Cleanup
    let _ = fs::remove_file(&path);

    match loaded {
        CellaConfig::D2(c) => {
            assert_eq!(c.width, 2);
            assert_eq!(c.height, 2);
            assert_eq!(c.history_limit, 3);
            assert_eq!(c.initial, init);
        }
        _ => panic!("Expected D2 config"),
    }
}

#[test]
fn test_build_grid1d_success() {
    let init = vec!["A".to_string(), "A".to_string()];
    let rule = Rule1D { subrules: vec![] };
    let cfg = CellaConfig::D1(Config1D {
        colors: Default::default(),
        seed: 0,
        ensemble: None,
        evolve: None,
        width: 2,
        history_limit: 2,
        initial: init,
        rule,
    });

    let g = cfg.build_grid1d().expect("Should build Grid1D");
    assert_eq!(g.width, 2);
    assert_eq!(g.cell_type(0).as_str(), "A");
}

#[test]
fn test_build_grid1d_fail_length() {
    let init = vec!["A".to_string()]; // Length 1
    let rule = Rule1D { subrules: vec![] };
    let cfg = CellaConfig::D1(Config1D {
        colors: Default::default(),
        seed: 0,
        ensemble: None,
        evolve: None,
        width: 2, // Width 2 -> Mismatch
        history_limit: 2,
        initial: init,
        rule,
    });

    assert!(
        cfg.build_grid1d().is_none(),
        "Should fail due to length mismatch"
    );
}

#[test]
fn test_build_grid1d_fail_wrong_dim() {
    let init = vec!["A".to_string()];
    let rule = Rule2D { subrules: vec![] };
    let cfg = CellaConfig::D2(Config2D {
        ensemble: None,
        evolve: None,
        seed: 0,
        colors: Default::default(),
        width: 1,
        height: 1,
        history_limit: 2,
        initial: init,
        rule,
        model: None,
    });

    assert!(
        cfg.build_grid1d().is_none(),
        "Should return None when building Grid1D from D2 config"
    );
}

#[test]
fn test_build_grid2d_success() {
    let init = vec![
        "A".to_string(),
        "B".to_string(),
        "C".to_string(),
        "D".to_string(),
    ];
    let rule = Rule2D { subrules: vec![] };
    let cfg = CellaConfig::D2(Config2D {
        ensemble: None,
        evolve: None,
        seed: 0,
        colors: Default::default(),
        width: 2,
        height: 2,
        history_limit: 2,
        initial: init,
        rule,
        model: None,
    });

    let g = cfg.build_grid2d().expect("Should build Grid2D");
    assert_eq!(g.width, 2);
    assert_eq!(g.height, 2);
    assert_eq!(g.cell_type(3).as_str(), "D");
}

#[test]
fn test_build_grid2d_fail_length() {
    let init = vec!["A".to_string()]; // Length 1
    let rule = Rule2D { subrules: vec![] };
    let cfg = CellaConfig::D2(Config2D {
        ensemble: None,
        evolve: None,
        seed: 0,
        colors: Default::default(),
        width: 2,
        height: 2, // Need 4
        history_limit: 2,
        initial: init,
        rule,
        model: None,
    });

    assert!(
        cfg.build_grid2d().is_none(),
        "Should fail due to length mismatch"
    );
}

#[test]
fn test_build_grid2d_fail_wrong_dim() {
    let init = vec!["A".to_string()];
    let rule = Rule1D { subrules: vec![] };
    let cfg = CellaConfig::D1(Config1D {
        colors: Default::default(),
        seed: 0,
        ensemble: None,
        evolve: None,
        width: 1,
        history_limit: 2,
        initial: init,
        rule,
    });

    assert!(
        cfg.build_grid2d().is_none(),
        "Should return None when building Grid2D from D1 config"
    );
}

#[test]
fn colors_parse_from_json_and_default_to_empty() {
    let json = r##"{"dim":"2d","width":2,"height":1,"history_limit":0,
        "initial":["A","B"],"rule":{"subrules":[]},
        "colors":{"A":"#2e8b57","B":"#ff4500"}}"##;
    let cfg: CellaConfig = serde_json::from_str(json).expect("parse");
    assert_eq!(cfg.colors().get("A").map(String::as_str), Some("#2e8b57"));
    assert_eq!(cfg.colors().get("B").map(String::as_str), Some("#ff4500"));

    let json_1d = r#"{"dim":"1d","width":3,"history_limit":0,"initial":["A","B","A"],
        "rule":{"subrules":[]}}"#;
    let cfg_1d: CellaConfig = serde_json::from_str(json_1d).expect("parse 1d");
    assert!(cfg_1d.colors().is_empty(), "missing field means no colours");
    // An empty map is left out again when written back.
    let back = serde_json::to_string(&cfg_1d).unwrap();
    assert!(!back.contains("colors"), "got {back}");
}

/// Every config shipped in `configs/` must load, build its grid, and build
/// whatever `ensemble` / `evolve` block it carries.
#[test]
fn every_shipped_config_loads_and_builds_its_blocks() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../configs");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).expect("configs directory") {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        seen += 1;
        let cfg = CellaConfig::from_file(&path)
            .unwrap_or_else(|e| panic!("{} does not load: {e}", path.display()));
        assert!(cfg.build_sim().is_some(), "{} does not build a grid", path.display());
        if let Some(r) = cfg.build_ensemble() {
            r.unwrap_or_else(|e| panic!("{} ensemble block: {e}", path.display()));
        }
        if let Some(r) = cfg.build_evolution() {
            r.unwrap_or_else(|e| panic!("{} evolve block: {e}", path.display()));
        }
    }
    assert!(seen >= 20, "expected the shipped configs, found {seen}");
}
