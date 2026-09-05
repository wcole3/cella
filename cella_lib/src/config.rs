//! Simple JSON configuration format to build grids without writing Rust code.
//! This format focuses on readability: you specify dimensions, history limit,
//! an initial array of type names, and the rule definition.
use crate::explore::{Ensemble, EnsembleConfig, Evolution, EvolveConfig, Sim};
use crate::external::ModelError;
use crate::grid1d::Grid1D;
use crate::grid2d::Grid2D;
use crate::rules::{Rule1D, Rule2D};
use crate::types::CellType;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Top-level configuration for either a 1D or 2D automaton.
///
/// You can serialize/deserialize this enum to exchange scenarios.
///
/// Example (build from JSON string)
/// ```rust
/// use cella_lib::config::{CellaConfig, Config2D};
/// use cella_lib::{Rule2D, Rule2DSubrule, Neighborhood2D};
/// let json = serde_json::json!({
///   "dim":"2d",
///   "width":3,
///   "height":3,
///   "history_limit":2,
///   "initial":["Inactive","Inactive","Inactive","Inactive","Alive","Inactive","Inactive","Inactive","Inactive"],
///   "rule":{
///     "subrules":[
///       {"current_type":"Alive","criteria_type":"Alive","count":4,"op":"gt","range":1,"neighborhood":"Moore","randomness":null,"output_type":"Inactive"},
///       {"current_type":"Alive","criteria_type":"Alive","count":2,"op":"gt","range":1,"neighborhood":"Moore","randomness":null,"output_type":"Alive"},
///       {"current_type":"Inactive","criteria_type":"Alive","count":3,"op":"eq","range":1,"neighborhood":"Moore","randomness":null,"output_type":"Alive"}
///     ]
///   }
/// });
/// let cfg: CellaConfig = serde_json::from_value(json).unwrap();
/// assert!(cfg.build_grid2d().is_some());
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "dim")]
pub enum CellaConfig {
    #[serde(rename = "1d")]
    D1(Config1D),
    #[serde(rename = "2d")]
    D2(Config2D),
}

impl CellaConfig {
    /// The `colors` map of whichever variant this is (empty when the file had none).
    pub fn colors(&self) -> &BTreeMap<String, String> {
        match self {
            CellaConfig::D1(c) => &c.colors,
            CellaConfig::D2(c) => &c.colors,
        }
    }
}

/// 1D configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config1D {
    /// Grid width.
    pub width: usize,
    /// Per-cell history cap.
    pub history_limit: usize,
    /// Initial cell types by name, length = width.
    pub initial: Vec<String>,
    /// Rule definition.
    pub rule: Rule1D,
    /// Seed for the rule's `randomness` draws (default 0). Same seed, same
    /// run, on any thread count. See [`crate::Grid1D::seed`].
    #[serde(default)]
    pub seed: u64,
    /// Display colours by cell-type name, as `#rrggbb` hex strings, e.g.
    /// `{"Forest": "#2e8b57"}`. Optional; the engine never reads them — the
    /// GUI applies them on load, and any type not listed gets an automatic
    /// colour. `"Inactive"` sets the background colour.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub colors: BTreeMap<String, String>,
    /// Optional ensemble settings; see [`crate::explore::ensemble`] and
    /// `docs/explore.md`. [`CellaConfig::build_ensemble`] uses it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ensemble: Option<EnsembleConfig>,
    /// Optional evolution settings; see [`crate::explore::evolve`] and
    /// `docs/explore.md`. [`CellaConfig::build_evolution`] uses it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evolve: Option<EvolveConfig>,
}

impl Default for Config1D {
    fn default() -> Self {
        Config1D {
            width: 0,
            history_limit: 0,
            initial: Vec::new(),
            rule: Rule1D {
                subrules: Vec::new(),
            },
            seed: 0,
            colors: BTreeMap::new(),
            ensemble: None,
            evolve: None,
        }
    }
}

/// 2D configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config2D {
    /// Grid width.
    pub width: usize,
    /// Grid height.
    pub height: usize,
    /// Per-cell history cap.
    pub history_limit: usize,
    /// Initial cell types by name, length = width*height.
    pub initial: Vec<String>,
    /// Rule definition.
    pub rule: Rule2D,
    /// Optional external transition model (e.g. `{"wildfire": {...}}`); when
    /// present it replaces the subrule engine. See [`crate::external`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<Box<dyn crate::external::ExternalModel>>,
    /// Seed for the rule's `randomness` draws (default 0). Same seed, same
    /// run, on any thread count. A model keeps its own seed; ensembles and
    /// evolutions reseed both per member. See [`crate::Grid2D::seed`].
    #[serde(default)]
    pub seed: u64,
    /// Display colours by cell-type name, as `#rrggbb` hex strings, e.g.
    /// `{"Forest": "#2e8b57"}`. Optional; the engine never reads them — the
    /// GUI applies them on load, and any type not listed gets an automatic
    /// colour. `"Inactive"` sets the background colour.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub colors: BTreeMap<String, String>,
    /// Optional ensemble settings (members, genes, tracked types, learning
    /// operators, driver); works with any rule or model. See
    /// [`crate::explore::ensemble`] and `docs/explore.md`.
    /// [`CellaConfig::build_ensemble`] uses it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ensemble: Option<EnsembleConfig>,
    /// Optional evolution settings (population, genes, objective, search
    /// mode). See [`crate::explore::evolve`] and `docs/explore.md`.
    /// [`CellaConfig::build_evolution`] uses it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evolve: Option<EvolveConfig>,
}

impl Default for Config2D {
    fn default() -> Self {
        Config2D {
            width: 0,
            height: 0,
            history_limit: 0,
            initial: Vec::new(),
            rule: Rule2D {
                subrules: Vec::new(),
            },
            model: None,
            seed: 0,
            colors: BTreeMap::new(),
            ensemble: None,
            evolve: None,
        }
    }
}

impl CellaConfig {
    /// Load configuration from a JSON file path.
    ///
    /// ```no_run
    /// use cella_lib::config::CellaConfig;
    /// let cfg = CellaConfig::from_file("configs/life.json").unwrap();
    /// ```
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let data = fs::read_to_string(path)?;
        let cfg: CellaConfig = serde_json::from_str(&data)?;
        Ok(cfg)
    }

    /// Save configuration to a JSON file path (pretty printed).
    ///
    /// ```no_run
    /// use cella_lib::config::{CellaConfig, Config2D};
    /// // write some cfg
    /// # let cfg: CellaConfig = serde_json::from_str("{\"dim\":\"2d\",\"width\":1,\"height\":1,\"history_limit\":1,\"initial\":[\"Inactive\"],\"rule\":{\"subrules\":[]}}").unwrap();
    /// cfg.to_file_pretty("out.json").unwrap();
    /// ```
    pub fn to_file_pretty<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let s = serde_json::to_string_pretty(self)?;
        fs::write(path, s)?;
        Ok(())
    }

    /// Build a Grid1D from D1 config.
    ///
    /// Returns `None` if `initial.len() != width`.
    pub fn build_grid1d(&self) -> Option<Grid1D> {
        match self {
            CellaConfig::D1(c) => {
                if c.initial.len() != c.width {
                    return None;
                }
                let init: Vec<CellType> = c.initial.iter().map(|s| CellType::new(s)).collect();
                Some(Grid1D::new(c.width, c.history_limit, init, c.rule.clone()).with_seed(c.seed))
            }
            _ => None,
        }
    }

    /// Build whichever grid this config describes, as a [`Sim`]. `None` on a
    /// length mismatch or a model that fails to attach.
    pub fn build_sim(&self) -> Option<Sim> {
        match self {
            CellaConfig::D1(_) => self.build_grid1d().map(Sim::D1),
            CellaConfig::D2(_) => self.build_grid2d().map(Sim::D2),
        }
    }

    /// The `ensemble` block, if the config has one.
    pub fn ensemble(&self) -> Option<&EnsembleConfig> {
        match self {
            CellaConfig::D1(c) => c.ensemble.as_ref(),
            CellaConfig::D2(c) => c.ensemble.as_ref(),
        }
    }

    /// The `evolve` block, if the config has one.
    pub fn evolve(&self) -> Option<&EvolveConfig> {
        match self {
            CellaConfig::D1(c) => c.evolve.as_ref(),
            CellaConfig::D2(c) => c.evolve.as_ref(),
        }
    }

    /// Build the ensemble this config describes: the grid is built (and its
    /// model attached) once, then cloned per member with genes drawn from the
    /// `ensemble.genes` ranges. `None` when the config has no `ensemble`
    /// block or the grid cannot be built; `Some(Err)` when the block is
    /// invalid (a gene names an unknown knob, a tracked type is not declared,
    /// a free gene has no driver, ...).
    pub fn build_ensemble(&self) -> Option<Result<Ensemble, ModelError>> {
        let cfg = self.ensemble()?;
        let sim = self.build_sim()?;
        Some(Ensemble::new(sim, cfg))
    }

    /// Build the evolution this config describes from its `evolve` block.
    /// `None` when there is no block or the grid cannot be built;
    /// `Some(Err)` when the block is invalid.
    pub fn build_evolution(&self) -> Option<Result<Evolution, ModelError>> {
        let cfg = self.evolve()?;
        let sim = self.build_sim()?;
        Some(Evolution::new(sim, cfg))
    }

    /// Build a Grid2D from D2 config.
    ///
    /// Returns `None` if `initial.len() != width*height`, or if the config's
    /// external model fails validation against the grid.
    pub fn build_grid2d(&self) -> Option<Grid2D> {
        match self {
            CellaConfig::D2(c) => {
                if c.initial.len() != c.width * c.height {
                    return None;
                }
                let init: Vec<CellType> = c.initial.iter().map(|s| CellType::new(s)).collect();
                let mut grid =
                    Grid2D::new(c.width, c.height, c.history_limit, init, c.rule.clone())
                        .with_seed(c.seed);
                if let Some(model) = &c.model {
                    grid.attach_model(model.clone()).ok()?;
                }
                Some(grid)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn build_grid_rejects_mismatched_initial_lengths() {
        let rule1 = Rule1D { subrules: vec![] };
        let cfg1 = CellaConfig::D1(Config1D {
            colors: Default::default(),
            seed: 0,
            ensemble: None,
            evolve: None,
            width: 3,
            history_limit: 1,
            initial: vec!["A".to_string(), "B".to_string()],
            rule: rule1,
        });
        assert!(cfg1.build_grid1d().is_none());

        let rule2 = Rule2D { subrules: vec![] };
        let cfg2 = CellaConfig::D2(Config2D {
            colors: Default::default(),
            width: 2,
            height: 2,
            history_limit: 1,
            initial: vec!["A".to_string(), "B".to_string(), "C".to_string()],
            rule: rule2,
            model: None,
            ensemble: None,
            evolve: None,
            seed: 0,
        });
        assert!(cfg2.build_grid2d().is_none());
    }

    #[test]
    fn file_roundtrip_and_build_grid_success_paths() {
        let a = "A".to_string();
        let b = "B".to_string();

        let cfg1 = CellaConfig::D1(Config1D {
            colors: Default::default(),
            seed: 0,
            ensemble: None,
            evolve: None,
            width: 2,
            history_limit: 1,
            initial: vec![a.clone(), b.clone()],
            rule: Rule1D { subrules: vec![] },
        });
        assert!(cfg1.build_grid1d().is_some());
        assert!(cfg1.build_grid2d().is_none());

        let cfg2 = CellaConfig::D2(Config2D {
            colors: Default::default(),
            width: 1,
            height: 2,
            history_limit: 1,
            initial: vec![a, b],
            rule: Rule2D { subrules: vec![] },
            model: None,
            ensemble: None,
            evolve: None,
            seed: 0,
        });
        assert!(cfg2.build_grid2d().is_some());
        assert!(cfg2.build_grid1d().is_none());

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("cella_cfg_{stamp}.json"));
        cfg2.to_file_pretty(&path).expect("write config");
        let loaded = CellaConfig::from_file(&path).expect("read config");
        assert!(matches!(loaded, CellaConfig::D2(_)));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn file_io_error_paths_are_covered() {
        let missing = std::env::temp_dir().join("cella_missing_config_hopefully.json");
        assert!(CellaConfig::from_file(&missing).is_err());

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let bad_path = std::env::temp_dir().join(format!("cella_bad_cfg_{stamp}.json"));
        std::fs::write(&bad_path, "{not json").unwrap();
        assert!(CellaConfig::from_file(&bad_path).is_err());
        let _ = std::fs::remove_file(&bad_path);

        let cfg = CellaConfig::D1(Config1D {
            colors: Default::default(),
            seed: 0,
            ensemble: None,
            evolve: None,
            width: 1,
            history_limit: 0,
            initial: vec!["Inactive".to_string()],
            rule: Rule1D { subrules: vec![] },
        });
        // Writing to a directory path fails, covering fs::write error propagation.
        let dir_path = std::env::temp_dir();
        assert!(cfg.to_file_pretty(dir_path).is_err());
    }
}
