//! One key grammar for every knob a grid has. A "knob" is any number or choice
//! you can adjust on a running grid, and its "key" is a string naming it.
//!
//! An external model already describes its parameters with [`ParamDesc`]
//! (see [`crate::external`]), which is what lets the GUI draw a slider for
//! `p0` without knowing what `p0` is. Rules had no such description: the
//! numbers inside a subrule (`count`, `range`, a Wolfram code) used to be
//! changeable only by editing the struct. This module gives them the same treatment,
//! and then joins both worlds behind a single key format so an optimiser, an
//! ensemble or a panel can say "set this knob" without caring whether the
//! knob belongs to the rule or to the model:
//!
//! ```text
//! rule.subrules[0].count        the count threshold of the first 2D subrule
//! rule.subrules[2].op           its comparison: "lt", "gt" or "eq"
//! rule.subrules[1].wolfram_code the bit table of the second 1D subrule
//! model.p0                      a parameter the attached model declares
//! ```
//!
//! Indices are zero-based, like the `subrules` array in a config file.
//!
//! Writing a rule field is never a bare field assignment. A 2D subrule keeps
//! derived data (neighbour offsets, the inclusive count window) that only
//! [`Rule2DSubrule::new`] knows how to compute, so a write copies the subrule,
//! changes one field, rebuilds it through `new`, runs `validate()`, and only
//! then swaps it in. A refused write leaves the rule exactly as it was.

use crate::external::{ModelError, ParamDesc, ParamKind, ParamValue, check_value_against_kind};
use crate::grid1d::Grid1D;
use crate::grid2d::Grid2D;
use crate::rules::{CountOp, Neighborhood2D, Rule1D, Rule2D, Rule2DSubrule};

/// Prefix of every model parameter key: `model.<key as the model names it>`.
pub const MODEL_PREFIX: &str = "model.";

/// Build the key for field `field` of subrule `index`, e.g.
/// `rule_key(2, "count")` is `"rule.subrules[2].count"`.
pub fn rule_key(index: usize, field: &str) -> String {
    format!("rule.subrules[{index}].{field}")
}

/// Split `rule.subrules[<i>].<field>` into `(i, field)`; `None` for any other
/// shape (including a `model.` key). Inverse of [`rule_key`].
pub fn parse_rule_key(key: &str) -> Option<(usize, &str)> {
    let rest = key.strip_prefix("rule.subrules[")?;
    let close = rest.find(']')?;
    let index: usize = rest[..close].parse().ok()?;
    let field = rest[close + 1..].strip_prefix('.')?;
    if field.is_empty() {
        return None;
    }
    Some((index, field))
}

fn invalid(key: &str, why: impl std::fmt::Display) -> ModelError {
    ModelError::InvalidParam(format!("'{key}': {why}"))
}

fn desc(key: String, label: String, group: String, help: &str, kind: ParamKind) -> ParamDesc {
    ParamDesc {
        key,
        label,
        group: Some(group),
        help: Some(help.to_string()),
        unit: None,
        kind,
        reattach: false,
        read_only: false,
    }
}

fn op_name(op: CountOp) -> &'static str {
    match op {
        CountOp::Lt => "lt",
        CountOp::Gt => "gt",
        CountOp::Eq => "eq",
    }
}

fn op_from_name(name: &str) -> Option<CountOp> {
    match name {
        "lt" => Some(CountOp::Lt),
        "gt" => Some(CountOp::Gt),
        "eq" => Some(CountOp::Eq),
        _ => None,
    }
}

fn neighborhood_name(n: Neighborhood2D) -> &'static str {
    match n {
        Neighborhood2D::Moore => "Moore",
        Neighborhood2D::VonNeumann => "VonNeumann",
        Neighborhood2D::Langton => "Langton",
        Neighborhood2D::StraightLine => "StraightLine",
        Neighborhood2D::Knight => "Knight",
    }
}

fn neighborhood_from_name(name: &str) -> Option<Neighborhood2D> {
    match name {
        "Moore" => Some(Neighborhood2D::Moore),
        "VonNeumann" => Some(Neighborhood2D::VonNeumann),
        "Langton" => Some(Neighborhood2D::Langton),
        "StraightLine" => Some(Neighborhood2D::StraightLine),
        "Knight" => Some(Neighborhood2D::Knight),
        _ => None,
    }
}

const NEIGHBORHOODS: [&str; 5] = ["Moore", "VonNeumann", "Langton", "StraightLine", "Knight"];
const OPS: [&str; 3] = ["lt", "gt", "eq"];
const FIELDS_1D: &str = "wolfram_code, randomness";
const FIELDS_2D: &str = "count, limit, range, op, neighborhood, randomness";

/// The control shape of one 1D subrule field, or `None` if the field does not
/// exist. Independent of whether the field is currently set, so a write can
/// turn `randomness` on.
fn kind_1d(sub: &crate::rules::Rule1DSubrule, field: &str) -> Option<ParamKind> {
    match field {
        "wolfram_code" => {
            if (1..=3).contains(&sub.n) {
                Some(ParamKind::Bits {
                    len: 1u32 << (2 * u32::from(sub.n) + 1),
                })
            } else {
                None
            }
        }
        "randomness" => Some(ParamKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.01,
        }),
        _ => None,
    }
}

/// Describe every tunable field of a 1D rule.
///
/// `wolfram_code` is a bit table of `2^(2n+1)` bits; `randomness` appears
/// only when the subrule has one (a subrule without randomness is not
/// stochastic, and a panel should not invite people to make it so by
/// accident — set it explicitly with [`set_rule1d_param`] if you want to).
pub fn rule1d_params(rule: &Rule1D) -> Vec<ParamDesc> {
    let mut out = Vec::new();
    for (i, sub) in rule.subrules.iter().enumerate() {
        let group = format!("Rule: subrule {}", i + 1);
        if let Some(kind) = kind_1d(sub, "wolfram_code") {
            out.push(desc(
                rule_key(i, "wolfram_code"),
                format!("Subrule {} code", i + 1),
                group.clone(),
                "Wolfram table: bit k is set when neighbourhood pattern k produces the output type.",
                kind,
            ));
        }
        if sub.randomness.is_some()
            && let Some(kind) = kind_1d(sub, "randomness")
        {
            out.push(desc(
                rule_key(i, "randomness"),
                format!("Subrule {} randomness", i + 1),
                group,
                "Probability that a matching subrule is skipped this step.",
                kind,
            ));
        }
    }
    out
}

/// Current value of a 1D rule field, or `None` for an unknown key or an unset
/// optional field.
pub fn get_rule1d_param(rule: &Rule1D, key: &str) -> Option<ParamValue> {
    let (i, field) = parse_rule_key(key)?;
    let sub = rule.subrules.get(i)?;
    match field {
        "wolfram_code" => Some(ParamValue::Bits(sub.wolfram_code)),
        "randomness" => sub.randomness.map(ParamValue::Float),
        _ => None,
    }
}

/// Write one 1D rule field. The value is checked against the field's kind,
/// then the changed subrule is validated; a refusal leaves the rule untouched.
pub fn set_rule1d_param(rule: &mut Rule1D, key: &str, value: ParamValue) -> Result<(), ModelError> {
    let (i, field) = parse_rule_key(key)
        .ok_or_else(|| invalid(key, format!("not a rule key (fields: {FIELDS_1D})")))?;
    let n = rule.subrules.len();
    let sub = rule
        .subrules
        .get(i)
        .ok_or_else(|| invalid(key, format!("rule has {n} subrules")))?;
    let kind = kind_1d(sub, field).ok_or_else(|| {
        invalid(
            key,
            format!("unknown field '{field}' (fields: {FIELDS_1D})"),
        )
    })?;
    check_value_against_kind(&kind, &value).map_err(|e| match e {
        ModelError::InvalidParam(why) => invalid(key, why),
        other => other,
    })?;
    let mut changed = sub.clone();
    match (field, &value) {
        ("wolfram_code", ParamValue::Bits(v)) => changed.wolfram_code = *v,
        ("randomness", ParamValue::Float(v)) => changed.randomness = Some(*v),
        _ => return Err(invalid(key, format!("cannot set '{field}' from {value:?}"))),
    }
    changed.validate().map_err(|e| invalid(key, e))?;
    rule.subrules[i] = changed;
    Ok(())
}

/// The control shape of one 2D subrule field, or `None` if the field does not
/// exist. `count` and `limit` are bounded by how many neighbours the
/// subrule's shape can ever see.
fn kind_2d(sub: &Rule2DSubrule, field: &str) -> Option<ParamKind> {
    let neighbours = sub.offsets.len() as i64;
    match field {
        "count" | "limit" => Some(ParamKind::Int {
            min: 0,
            max: neighbours,
        }),
        "range" => Some(ParamKind::Int { min: 1, max: 8 }),
        "op" => Some(ParamKind::Choice {
            options: OPS.iter().map(|s| s.to_string()).collect(),
        }),
        "neighborhood" => Some(ParamKind::Choice {
            options: NEIGHBORHOODS.iter().map(|s| s.to_string()).collect(),
        }),
        "randomness" => Some(ParamKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.01,
        }),
        _ => None,
    }
}

/// Describe every tunable field of a 2D rule: `count`, `range`, `op` and
/// `neighborhood` for each subrule, plus `limit` and `randomness` when set.
pub fn rule2d_params(rule: &Rule2D) -> Vec<ParamDesc> {
    let mut out = Vec::new();
    for (i, sub) in rule.subrules.iter().enumerate() {
        let group = format!("Rule: subrule {}", i + 1);
        let n = i + 1;
        let push = |out: &mut Vec<ParamDesc>, field: &str, label: &str, help: &str| {
            if let Some(kind) = kind_2d(sub, field) {
                out.push(desc(
                    rule_key(i, field),
                    format!("Subrule {n} {label}"),
                    group.clone(),
                    help,
                    kind,
                ));
            }
        };
        push(
            &mut out,
            "count",
            "count",
            "Neighbour count the comparison is made against.",
        );
        if sub.limit.is_some() {
            push(
                &mut out,
                "limit",
                "limit",
                "Second bound: with gt the count must be in [count, limit]; with lt in [limit, count].",
            );
        }
        push(
            &mut out,
            "range",
            "range",
            "Neighbourhood radius; the window is (2·range+1) cells across.",
        );
        push(
            &mut out,
            "op",
            "op",
            "Comparison: gt means at least count, lt at most count, eq exactly count.",
        );
        push(
            &mut out,
            "neighborhood",
            "neighborhood",
            "Which cells around the centre are counted.",
        );
        if sub.randomness.is_some() {
            push(
                &mut out,
                "randomness",
                "randomness",
                "Probability that a matching subrule is skipped this step.",
            );
        }
    }
    out
}

/// Current value of a 2D rule field, or `None` for an unknown key or an unset
/// optional field.
pub fn get_rule2d_param(rule: &Rule2D, key: &str) -> Option<ParamValue> {
    let (i, field) = parse_rule_key(key)?;
    let sub = rule.subrules.get(i)?;
    match field {
        "count" => Some(ParamValue::Int(i64::from(sub.count))),
        "limit" => sub.limit.map(|l| ParamValue::Int(i64::from(l))),
        "range" => Some(ParamValue::Int(i64::from(sub.range))),
        "op" => Some(ParamValue::Choice(op_name(sub.op).to_string())),
        "neighborhood" => Some(ParamValue::Choice(
            neighborhood_name(sub.neighborhood).to_string(),
        )),
        "randomness" => sub.randomness.map(ParamValue::Float),
        _ => None,
    }
}

/// Write one 2D rule field.
///
/// The value is checked against the field's kind, the subrule is rebuilt
/// through [`Rule2DSubrule::new`] so its derived data (offsets, the count
/// window) matches the new field, and `validate()` runs on the result. A
/// refusal — an unknown key, a value of the wrong shape or out of bounds, or
/// a combination the rule engine rejects such as `op = eq` together with a
/// `limit` — leaves the rule untouched.
pub fn set_rule2d_param(rule: &mut Rule2D, key: &str, value: ParamValue) -> Result<(), ModelError> {
    let (i, field) = parse_rule_key(key)
        .ok_or_else(|| invalid(key, format!("not a rule key (fields: {FIELDS_2D})")))?;
    let n = rule.subrules.len();
    let sub = rule
        .subrules
        .get(i)
        .ok_or_else(|| invalid(key, format!("rule has {n} subrules")))?;
    let kind = kind_2d(sub, field).ok_or_else(|| {
        invalid(
            key,
            format!("unknown field '{field}' (fields: {FIELDS_2D})"),
        )
    })?;
    check_value_against_kind(&kind, &value).map_err(|e| match e {
        ModelError::InvalidParam(why) => invalid(key, why),
        other => other,
    })?;
    let (mut count, mut op, mut range, mut neighborhood, mut randomness, mut limit) = (
        sub.count,
        sub.op,
        sub.range,
        sub.neighborhood,
        sub.randomness,
        sub.limit,
    );
    match (field, &value) {
        ("count", ParamValue::Int(v)) => count = *v as u32,
        ("limit", ParamValue::Int(v)) => limit = Some(*v as u32),
        ("range", ParamValue::Int(v)) => range = *v as u8,
        ("op", ParamValue::Choice(name)) => {
            op = op_from_name(name).ok_or_else(|| invalid(key, format!("unknown op '{name}'")))?;
        }
        ("neighborhood", ParamValue::Choice(name)) => {
            neighborhood = neighborhood_from_name(name)
                .ok_or_else(|| invalid(key, format!("unknown neighborhood '{name}'")))?;
        }
        ("randomness", ParamValue::Float(v)) => randomness = Some(*v),
        _ => return Err(invalid(key, format!("cannot set '{field}' from {value:?}"))),
    }
    let rebuilt = Rule2DSubrule::new(
        sub.current_type,
        sub.criteria_type,
        count,
        op,
        range,
        neighborhood,
        sub.output_type,
        randomness,
        limit,
    );
    rebuilt.validate().map_err(|e| invalid(key, e))?;
    rule.subrules[i] = rebuilt;
    Ok(())
}

impl Grid1D {
    /// Every tunable knob of this grid, described for a generic control or an
    /// optimiser. A 1D grid has only rule fields; see [`rule1d_params`].
    pub fn params(&self) -> Vec<ParamDesc> {
        rule1d_params(&self.rule)
    }

    /// Current value of one knob by key, or `None` if there is no such knob.
    pub fn get_param(&self, key: &str) -> Option<ParamValue> {
        get_rule1d_param(&self.rule, key)
    }

    /// Set one knob by key; see [`set_rule1d_param`] for what is refused.
    pub fn set_param(&mut self, key: &str, value: ParamValue) -> Result<(), ModelError> {
        set_rule1d_param(&mut self.rule, key, value)
    }
}

impl Grid2D {
    /// Every tunable knob of this grid: the rule fields ([`rule2d_params`])
    /// followed by the attached model's parameters with their keys prefixed
    /// `model.` (the model's own groups and labels are kept).
    pub fn params(&self) -> Vec<ParamDesc> {
        let mut out = rule2d_params(&self.rule);
        if let Some(model) = &self.model {
            out.extend(model.params().into_iter().map(|mut d| {
                d.key = format!("{MODEL_PREFIX}{}", d.key);
                d
            }));
        }
        out
    }

    /// Current value of one knob by key, or `None` if there is no such knob.
    pub fn get_param(&self, key: &str) -> Option<ParamValue> {
        match key.strip_prefix(MODEL_PREFIX) {
            Some(model_key) => self.model.as_ref()?.get_param(model_key),
            None => get_rule2d_param(&self.rule, key),
        }
    }

    /// Set one knob by key. Rule keys go through [`set_rule2d_param`]; model
    /// keys through [`Grid2D::set_model_param`], which validates, re-attaches
    /// when the model asks for it, and rolls back on failure.
    pub fn set_param(&mut self, key: &str, value: ParamValue) -> Result<(), ModelError> {
        match key.strip_prefix(MODEL_PREFIX) {
            Some(model_key) => self.set_model_param(model_key, value),
            None => set_rule2d_param(&mut self.rule, key, value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::Rule1DSubrule;
    use crate::types::CellType;

    fn life() -> Rule2D {
        let alive = CellType::from("Alive");
        let dead = CellType::inactive();
        let sub = |cur, count, op, limit, out| {
            Rule2DSubrule::new(
                cur,
                alive,
                count,
                op,
                1,
                Neighborhood2D::Moore,
                out,
                None,
                limit,
            )
        };
        Rule2D {
            subrules: vec![
                sub(alive, 4, CountOp::Gt, None, dead),
                sub(alive, 2, CountOp::Gt, Some(3), alive),
                sub(dead, 3, CountOp::Eq, None, alive),
            ],
        }
    }

    fn rule30() -> Rule1D {
        let x = CellType::from("X");
        let mk = |cur, r| Rule1DSubrule {
            current_type: cur,
            criteria_type: x,
            wolfram_code: 30,
            n: 1,
            randomness: r,
            output_type: x,
        };
        Rule1D {
            subrules: vec![mk(x, None), mk(CellType::inactive(), Some(0.25))],
        }
    }

    #[test]
    fn keys_round_trip_and_bad_shapes_are_rejected() {
        assert_eq!(rule_key(2, "count"), "rule.subrules[2].count");
        assert_eq!(parse_rule_key("rule.subrules[2].count"), Some((2, "count")));
        assert_eq!(
            parse_rule_key("rule.subrules[0].wolfram_code"),
            Some((0, "wolfram_code"))
        );
        for bad in [
            "rule.subrules[x].count",
            "rule.subrules[0]",
            "rule.subrules[0].",
            "rule.subrules0.count",
            "model.p0",
            "count",
        ] {
            assert_eq!(parse_rule_key(bad), None, "{bad}");
        }
    }

    #[test]
    fn rule1d_params_list_codes_always_and_randomness_only_when_set() {
        let rule = rule30();
        let descs = rule1d_params(&rule);
        let keys: Vec<&str> = descs.iter().map(|d| d.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "rule.subrules[0].wolfram_code",
                "rule.subrules[1].wolfram_code",
                "rule.subrules[1].randomness",
            ]
        );
        assert_eq!(descs[0].kind, ParamKind::Bits { len: 8 });
        assert!(descs.iter().all(|d| !d.read_only && !d.reattach));
        assert_eq!(
            get_rule1d_param(&rule, "rule.subrules[0].wolfram_code"),
            Some(ParamValue::Bits(30))
        );
        assert_eq!(
            get_rule1d_param(&rule, "rule.subrules[1].randomness"),
            Some(ParamValue::Float(0.25))
        );
        assert_eq!(get_rule1d_param(&rule, "rule.subrules[0].randomness"), None);
        assert_eq!(
            get_rule1d_param(&rule, "rule.subrules[7].wolfram_code"),
            None
        );
        assert_eq!(get_rule1d_param(&rule, "rule.subrules[0].nope"), None);
    }

    #[test]
    fn rule1d_writes_are_validated_and_all_or_nothing() {
        let mut rule = rule30();
        set_rule1d_param(
            &mut rule,
            "rule.subrules[0].wolfram_code",
            ParamValue::Bits(110),
        )
        .unwrap();
        assert_eq!(rule.subrules[0].wolfram_code, 110);
        // 256 needs 9 bits; an n=1 table has 8.
        let err = set_rule1d_param(
            &mut rule,
            "rule.subrules[0].wolfram_code",
            ParamValue::Bits(256),
        )
        .unwrap_err();
        assert!(format!("{err}").contains("does not fit in 8 bits"), "{err}");
        assert_eq!(
            rule.subrules[0].wolfram_code, 110,
            "refused write changed nothing"
        );
        // Randomness can be switched on by a write even though it was not listed.
        set_rule1d_param(
            &mut rule,
            "rule.subrules[0].randomness",
            ParamValue::Float(0.5),
        )
        .unwrap();
        assert_eq!(rule.subrules[0].randomness, Some(0.5));
        assert!(
            set_rule1d_param(
                &mut rule,
                "rule.subrules[0].randomness",
                ParamValue::Float(1.5)
            )
            .is_err()
        );
        assert!(
            set_rule1d_param(
                &mut rule,
                "rule.subrules[9].wolfram_code",
                ParamValue::Bits(1)
            )
            .is_err()
        );
        assert!(set_rule1d_param(&mut rule, "rule.subrules[0].count", ParamValue::Int(1)).is_err());
        assert!(set_rule1d_param(&mut rule, "model.p0", ParamValue::Float(0.1)).is_err());
        // Wrong value shape for a known field.
        assert!(
            set_rule1d_param(
                &mut rule,
                "rule.subrules[0].wolfram_code",
                ParamValue::Int(3)
            )
            .is_err()
        );
    }

    #[test]
    fn rule2d_params_describe_each_subrule_with_bounds_from_its_shape() {
        let rule = life();
        let descs = rule2d_params(&rule);
        let keys: Vec<&str> = descs.iter().map(|d| d.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "rule.subrules[0].count",
                "rule.subrules[0].range",
                "rule.subrules[0].op",
                "rule.subrules[0].neighborhood",
                "rule.subrules[1].count",
                "rule.subrules[1].limit",
                "rule.subrules[1].range",
                "rule.subrules[1].op",
                "rule.subrules[1].neighborhood",
                "rule.subrules[2].count",
                "rule.subrules[2].range",
                "rule.subrules[2].op",
                "rule.subrules[2].neighborhood",
            ]
        );
        // Moore radius 1 sees 8 neighbours, so count is bounded by 8.
        assert_eq!(descs[0].kind, ParamKind::Int { min: 0, max: 8 });
        assert_eq!(descs[0].group.as_deref(), Some("Rule: subrule 1"));
        assert_eq!(
            get_rule2d_param(&rule, "rule.subrules[1].limit"),
            Some(ParamValue::Int(3))
        );
        assert_eq!(
            get_rule2d_param(&rule, "rule.subrules[2].op"),
            Some(ParamValue::Choice("eq".into()))
        );
        assert_eq!(
            get_rule2d_param(&rule, "rule.subrules[0].neighborhood"),
            Some(ParamValue::Choice("Moore".into()))
        );
        assert_eq!(get_rule2d_param(&rule, "rule.subrules[0].limit"), None);
        assert_eq!(get_rule2d_param(&rule, "rule.subrules[0].randomness"), None);
    }

    #[test]
    fn rule2d_writes_rebuild_derived_fields_exactly_like_new() {
        let mut rule = life();
        let alive = CellType::from("Alive");
        set_rule2d_param(&mut rule, "rule.subrules[2].count", ParamValue::Int(5)).unwrap();
        set_rule2d_param(&mut rule, "rule.subrules[2].range", ParamValue::Int(2)).unwrap();
        set_rule2d_param(
            &mut rule,
            "rule.subrules[2].neighborhood",
            ParamValue::Choice("VonNeumann".into()),
        )
        .unwrap();
        let fresh = Rule2DSubrule::new(
            CellType::inactive(),
            alive,
            5,
            CountOp::Eq,
            2,
            Neighborhood2D::VonNeumann,
            alive,
            None,
            None,
        );
        let got = &rule.subrules[2];
        assert_eq!(got.offsets, fresh.offsets);
        assert_eq!(got.pad, fresh.pad);
        assert_eq!(got.pad, 2);
        assert_eq!((got.cond_lo, got.cond_hi), (fresh.cond_lo, fresh.cond_hi));
        assert_eq!(got.early_exit, fresh.early_exit);
        // gt without a limit is the early-exit shape; switching op changes it.
        assert!(rule.subrules[0].early_exit);
        set_rule2d_param(
            &mut rule,
            "rule.subrules[0].op",
            ParamValue::Choice("lt".into()),
        )
        .unwrap();
        assert!(!rule.subrules[0].early_exit);
        assert_eq!((rule.subrules[0].cond_lo, rule.subrules[0].cond_hi), (0, 4));
        // A limit can be switched on, and randomness too.
        set_rule2d_param(&mut rule, "rule.subrules[0].limit", ParamValue::Int(2)).unwrap();
        assert_eq!(rule.subrules[0].limit, Some(2));
        set_rule2d_param(
            &mut rule,
            "rule.subrules[0].randomness",
            ParamValue::Float(0.1),
        )
        .unwrap();
        assert_eq!(rule.subrules[0].randomness, Some(0.1));
        assert!(
            rule2d_params(&rule)
                .iter()
                .any(|d| d.key == "rule.subrules[0].randomness")
        );
    }

    #[test]
    fn rule2d_refusals_leave_the_rule_untouched() {
        let mut rule = life();
        let before = rule.clone();
        // eq + limit is a combination the engine rejects.
        let err =
            set_rule2d_param(&mut rule, "rule.subrules[2].limit", ParamValue::Int(4)).unwrap_err();
        assert!(
            format!("{err}").contains("range must be >= 1")
                || format!("{err}").contains("subrules[2]"),
            "{err}"
        );
        // count above what the shape can see.
        assert!(set_rule2d_param(&mut rule, "rule.subrules[0].count", ParamValue::Int(9)).is_err());
        // limit below count for gt.
        assert!(set_rule2d_param(&mut rule, "rule.subrules[1].limit", ParamValue::Int(1)).is_err());
        // unknown op / neighborhood names, wrong shapes, unknown fields, bad index.
        assert!(
            set_rule2d_param(
                &mut rule,
                "rule.subrules[0].op",
                ParamValue::Choice("ge".into())
            )
            .is_err()
        );
        assert!(
            set_rule2d_param(
                &mut rule,
                "rule.subrules[0].neighborhood",
                ParamValue::Choice("Hex".into())
            )
            .is_err()
        );
        assert!(
            set_rule2d_param(&mut rule, "rule.subrules[0].count", ParamValue::Float(1.0)).is_err()
        );
        assert!(
            set_rule2d_param(
                &mut rule,
                "rule.subrules[0].wolfram_code",
                ParamValue::Bits(1)
            )
            .is_err()
        );
        assert!(set_rule2d_param(&mut rule, "rule.subrules[3].count", ParamValue::Int(1)).is_err());
        assert!(set_rule2d_param(&mut rule, "nonsense", ParamValue::Int(1)).is_err());
        assert_eq!(rule.subrules, before.subrules);
    }

    #[test]
    fn grids_expose_rule_and_model_knobs_behind_one_key_format() {
        let x = CellType::from("X");
        let mut g1 = Grid1D::new(5, 0, vec![x; 5], rule30());
        assert_eq!(g1.params().len(), 3);
        g1.set_param("rule.subrules[0].wolfram_code", ParamValue::Bits(90))
            .unwrap();
        assert_eq!(
            g1.get_param("rule.subrules[0].wolfram_code"),
            Some(ParamValue::Bits(90))
        );
        assert!(g1.set_param("model.p0", ParamValue::Float(0.1)).is_err());

        let alive = CellType::from("Alive");
        let mut g2 = Grid2D::new(3, 3, 0, vec![alive; 9], life());
        assert_eq!(g2.params().len(), 13, "no model: rule fields only");
        assert_eq!(g2.get_param("model.p0"), None);
        assert!(g2.set_param("model.p0", ParamValue::Float(0.2)).is_err());
        g2.set_param("rule.subrules[0].count", ParamValue::Int(3))
            .unwrap();
        assert_eq!(
            g2.get_param("rule.subrules[0].count"),
            Some(ParamValue::Int(3))
        );

        // With a model attached its parameters appear under `model.` and are
        // written through the validating, rolling-back model path.
        let forest = CellType::from("Forest");
        let mut g3 = Grid2D::new(4, 4, 0, vec![forest; 16], Rule2D { subrules: vec![] });
        let model = crate::wildfire::WildfireModel::new(
            crate::wildfire::WildfireParams {
                seed: 1,
                p0: 0.3,
                fuels: vec![crate::wildfire::FuelClass {
                    name: "Forest".into(),
                    veg_factor: 1.0,
                }],
                wind_speed: 0.0,
                wind_from_deg: 0.0,
                c1: 0.045,
                c2: 0.131,
                slope_a: 0.078,
                cell_size: 30.0,
                burn_duration: 1,
                spotting: None,
                burning_name: None,
                burned_name: None,
                spread: "bernoulli".into(),
                arrival_jitter: 0.2,
                wind_law: "exponential".into(),
            },
            crate::wildfire::WildfireEnv::default(),
        );
        g3.attach_model(Box::new(model)).unwrap();
        let descs = g3.params();
        assert!(descs.iter().any(|d| d.key == "model.p0"));
        assert!(
            descs.iter().all(|d| d.key.starts_with("model.")),
            "empty rule, only model keys"
        );
        assert_eq!(g3.get_param("model.p0"), Some(ParamValue::Float(0.3)));
        g3.set_param("model.p0", ParamValue::Float(0.4)).unwrap();
        assert_eq!(g3.get_param("model.p0"), Some(ParamValue::Float(0.4)));
        assert!(
            g3.set_param("model.p0", ParamValue::Float(7.0)).is_err(),
            "out of the model's bounds"
        );
        assert_eq!(
            g3.get_param("model.p0"),
            Some(ParamValue::Float(0.4)),
            "refused write rolled back"
        );
        assert!(
            g3.set_param("model.seed", ParamValue::Int(5)).is_err(),
            "read-only model key"
        );
    }

    #[test]
    fn op_and_neighborhood_names_round_trip_every_variant() {
        for (op, name) in [
            (CountOp::Lt, "lt"),
            (CountOp::Gt, "gt"),
            (CountOp::Eq, "eq"),
        ] {
            assert_eq!(op_name(op), name);
            assert_eq!(op_from_name(name), Some(op));
        }
        assert_eq!(op_from_name("nope"), None);

        for (n, name) in [
            (Neighborhood2D::Moore, "Moore"),
            (Neighborhood2D::VonNeumann, "VonNeumann"),
            (Neighborhood2D::Langton, "Langton"),
            (Neighborhood2D::StraightLine, "StraightLine"),
            (Neighborhood2D::Knight, "Knight"),
        ] {
            assert_eq!(neighborhood_name(n), name);
            assert_eq!(neighborhood_from_name(name), Some(n));
        }
        assert_eq!(neighborhood_from_name("nope"), None);
    }

    #[test]
    fn kind_1d_has_no_wolfram_code_kind_outside_n_1_to_3_and_a_bogus_2d_field_is_none() {
        // n=0 is outside the 1..=3 window `wolfram_code`'s bit width needs;
        // `kind_1d` reports it as absent rather than picking an arbitrary
        // width (the same "field does not exist right now" convention
        // `kind_1d`'s doc describes).
        let x = CellType::from("X");
        let out_of_range = Rule1DSubrule {
            current_type: x,
            criteria_type: x,
            wolfram_code: 2,
            n: 0,
            randomness: None,
            output_type: x,
        };
        assert_eq!(kind_1d(&out_of_range, "wolfram_code"), None);
        // randomness does not depend on n, so it is still a real field.
        assert!(kind_1d(&out_of_range, "randomness").is_some());

        // A field name `get_rule2d_param` has never heard of comes back
        // `None`, the same as an unset optional field but for a different
        // reason (this one distinguishes "no such field" from the
        // `randomness`-unset case the acceptance test above already covers).
        let rule = life();
        assert_eq!(get_rule2d_param(&rule, "rule.subrules[0].bogus"), None);
    }
}
