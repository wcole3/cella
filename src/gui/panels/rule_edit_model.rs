//! The rule editor's working copy of a rule.
//!
//! The editor cannot edit a `Rule1D`/`Rule2D` in place: the user types partial
//! values (an empty Wolfram code, a half-typed number) that would not validate.
//! So the editor keeps everything as `String`s here, and only converts to a real
//! rule when "Apply to grid" is pressed — which is what `to_rule` does, and why
//! it returns a `Result`.

use cella_lib::*;

#[derive(Clone, Debug)]
pub(in crate::gui) struct Rule1DSubruleEdit {
    pub(in crate::gui) current: String,
    pub(in crate::gui) criteria: String,
    pub(in crate::gui) wolfram_code: String,
    pub(in crate::gui) n: u8,
    pub(in crate::gui) randomness_enabled: bool,
    pub(in crate::gui) randomness_value: f64,
    pub(in crate::gui) output: String,
}

#[derive(Clone, Debug)]
pub(in crate::gui) struct Rule1DEdit {
    pub(in crate::gui) subrules: Vec<Rule1DSubruleEdit>,
}

impl Rule1DEdit {
    pub(in crate::gui) fn from_rule(rule: &Rule1D) -> Self {
        let subs = rule
            .subrules
            .iter()
            .map(|s| Rule1DSubruleEdit {
                current: s.current_type.as_str().to_string(),
                criteria: s.criteria_type.as_str().to_string(),
                wolfram_code: s.wolfram_code.to_string(),
                n: s.n,
                randomness_enabled: s.randomness.is_some(),
                randomness_value: s.randomness.unwrap_or(0.0),
                output: s.output_type.as_str().to_string(),
            })
            .collect();
        Self { subrules: subs }
    }
    pub(in crate::gui) fn to_rule(&self) -> Result<Rule1D, String> {
        let mut subs: Vec<Rule1DSubrule> = Vec::new();
        for s in &self.subrules {
            let code = s
                .wolfram_code
                .trim()
                .parse::<u128>()
                .map_err(|e| format!("wolfram_code parse error: {}", e))?;
            let randomness = if s.randomness_enabled {
                Some(s.randomness_value)
            } else {
                None
            };
            let sub = Rule1DSubrule {
                current_type: CellType::from(s.current.clone()),
                criteria_type: CellType::from(s.criteria.clone()),
                wolfram_code: code,
                n: s.n,
                randomness,
                output_type: CellType::from(s.output.clone()),
            };
            if let Err(e) = sub.validate() {
                return Err(format!("validation error: {}", e));
            }
            subs.push(sub);
        }
        Ok(Rule1D { subrules: subs })
    }
}

#[derive(Clone, Debug)]
pub(in crate::gui) struct Rule2DSubruleEdit {
    pub(in crate::gui) current: String,
    pub(in crate::gui) criteria: String,
    pub(in crate::gui) count: u32,
    pub(in crate::gui) op: CountOp,
    pub(in crate::gui) limit_enabled: bool,
    pub(in crate::gui) limit_value: u32,
    pub(in crate::gui) range: u8,
    pub(in crate::gui) neighborhood: Neighborhood2D,
    pub(in crate::gui) randomness_enabled: bool,
    pub(in crate::gui) randomness_value: f64,
    pub(in crate::gui) output: String,
}

#[derive(Clone, Debug)]
pub(in crate::gui) struct Rule2DEdit {
    pub(in crate::gui) subrules: Vec<Rule2DSubruleEdit>,
}

impl Rule2DEdit {
    pub(in crate::gui) fn from_rule(rule: &Rule2D) -> Self {
        let subs = rule
            .subrules
            .iter()
            .map(|s| Rule2DSubruleEdit {
                current: s.current_type.as_str().to_string(),
                criteria: s.criteria_type.as_str().to_string(),
                count: s.count,
                op: s.op,
                limit_enabled: s.limit.is_some(),
                limit_value: s.limit.unwrap_or(0),
                range: s.range,
                neighborhood: s.neighborhood,
                randomness_enabled: s.randomness.is_some(),
                randomness_value: s.randomness.unwrap_or(0.0),
                output: s.output_type.as_str().to_string(),
            })
            .collect();
        Self { subrules: subs }
    }
    pub(in crate::gui) fn to_rule(&self) -> Result<Rule2D, String> {
        let mut subs: Vec<Rule2DSubrule> = Vec::new();
        for s in &self.subrules {
            let randomness = if s.randomness_enabled {
                Some(s.randomness_value)
            } else {
                None
            };
            let limit = if s.limit_enabled {
                Some(s.limit_value)
            } else {
                None
            };
            let sub = Rule2DSubrule::new(
                CellType::from(s.current.clone()),
                CellType::from(s.criteria.clone()),
                s.count,
                s.op,
                s.range,
                s.neighborhood,
                CellType::from(s.output.clone()),
                randomness,
                limit,
            );
            if let Err(e) = sub.validate() {
                return Err(format!("validation error: {}", e));
            }
            subs.push(sub);
        }
        Ok(Rule2D { subrules: subs })
    }
}
