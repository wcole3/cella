//! Simple JSON configuration format to build grids without writing Rust code.
//! This format focuses on readability: you specify dimensions, history limit,
//! an initial array of type names, and the rule definition.
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use crate::types::CellType;
use crate::rules::{Rule1D, Rule2D};
use crate::grid1d::Grid1D;
use crate::grid2d::Grid2D;

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
///       {"current_type":"Alive","criteria_type":"Alive","threshold":2,"range":1,"neighborhood":"Moore","randomness":null,"output_type":"Alive"},
///       {"current_type":"Inactive","criteria_type":"Alive","threshold":3,"range":1,"neighborhood":"Moore","randomness":null,"output_type":"Alive"}
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
    pub fn to_file_pretty<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
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
                if c.initial.len() != c.width { return None; }
                let init: Vec<CellType> = c.initial.iter().map(|s| CellType(s.clone())).collect();
                Some(Grid1D::new(c.width, c.history_limit, init, c.rule.clone()))
            }
            _ => None,
        }
    }

    /// Build a Grid2D from D2 config.
    ///
    /// Returns `None` if `initial.len() != width*height`.
    pub fn build_grid2d(&self) -> Option<Grid2D> {
        match self {
            CellaConfig::D2(c) => {
                if c.initial.len() != c.width * c.height { return None; }
                let init: Vec<CellType> = c.initial.iter().map(|s| CellType(s.clone())).collect();
                Some(Grid2D::new(c.width, c.height, c.history_limit, init, c.rule.clone()))
            }
            _ => None,
        }
    }
}
