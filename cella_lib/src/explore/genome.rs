//! Genes and genomes: which knobs vary, over what range, and how they change.
//!
//! A **gene** is one knob that is allowed to vary, with the range it may take.
//! A **genome** is one setting for every gene — the thing an ensemble member
//! or an evolved individual carries. The [`GeneSpace`] holds the genes and
//! knows how to draw a fresh genome, nudge one, cross two, and write one into
//! a grid.
//!
//! Genes come from a config as [`GeneSpec`]s, one flat object each:
//!
//! ```json
//! {"key": "model.p0", "range": [0.08, 0.6], "scale": "log"}
//! {"key": "rule.subrules[1].limit", "range": [2, 8]}
//! {"key": "rule.subrules[*].wolfram_code", "bits": 128, "sigma": 0.01}
//! {"key": "rule.subrules[2].op", "choices": ["eq", "gt"]}
//! {"key": "wind_scale", "range": [0.0, 1.5]}
//! ```
//!
//! - A key with a `rule.` or `model.` prefix names a knob the grid describes
//!   (see [`crate::tunables`]). Its kind and bounds come from that description;
//!   `range`/`bits`/`choices` may only *narrow* them and can be left out.
//!   `[*]` in place of a subrule index writes the same value into every
//!   subrule that has the field.
//! - A key with no prefix is a **free gene**: the grid does not know it, a
//!   [driver](super::driver) does (a wind multiplier, a decay time). It needs
//!   a `range`, `bits` or `choices`, or a default kind from the driver.
//!
//! One rule for mutation size: **`sigma` is a fraction of the gene's size.**
//! A number moves by a Gaussian step with standard deviation `sigma × (hi −
//! lo)` (on a log scale for `scale: log`), then is clamped; a switch flips
//! with probability `sigma`; a choice is redrawn with probability `sigma`;
//! in a bit-string about `8 × sigma` bits flip whatever its length (one or
//! two at the default 0.2). A per-gene `sigma` overrides the engine's.
//! `sigma: 0` is allowed for a per-gene override (unlike the engine's own
//! `sigma`, which must stay positive) and freezes that one gene: every
//! Gaussian step, flip and redraw above becomes a no-op at `sigma = 0`, so
//! a member keeps exactly the value it was born with — selection can still
//! act on that value (resampling still copies it), only mutation stops.
//!
//! **Exception: `evolve`'s iso+line variation.** The freeze above describes
//! [`GeneSpace::mutate`]/`mutate_one`, the path `assim`/`open` mode (and
//! resampling everywhere) use. `evolve` mode's own crossover-flavoured step
//! ([`crate::explore::evolve::iso_line_step`]) does not honour a per-gene
//! `sigma: 0` freeze the same way: alongside its own Gaussian step (scaled
//! by the gene's `sigma`, so that half does stop at `sigma = 0`), it also
//! moves the child a fraction of the way along the line toward a second,
//! paired parent — a step whose size is a fixed 0.2 spread, independent of
//! the gene's own `sigma`. A gene frozen this way in `evolve` mode can
//! still move, via the line term alone, even at `sigma = 0`. No behaviour
//! change here — this paragraph only documents a gap this module's own
//! doc comment did not previously mention.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::sim::Sim;
use crate::external::{ModelError, ParamDesc, ParamKind, ParamValue};
use crate::rng::Rng;
use crate::tunables::{MODEL_PREFIX, parse_rule_key, rule_key};

/// How a numeric range is sampled and mutated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scale {
    /// Every value in the range is equally likely.
    #[default]
    Linear,
    /// Every *decade* is equally likely; needs `lo > 0`. Use it for knobs
    /// that span orders of magnitude (a probability from 0.001 to 0.5, a
    /// time-scale from 2 to 100 days).
    Log,
}

/// One gene as written in a config file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneSpec {
    /// Which knob: `rule.subrules[i].field`, `model.key`, or a free name.
    pub key: String,
    /// `[lo, hi]` for a numeric gene. Integer knobs need whole numbers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<[f64; 2]>,
    /// Linear (default) or log sampling for a numeric gene.
    #[serde(default, skip_serializing_if = "is_linear")]
    pub scale: Scale,
    /// Width of a bit-string gene (`wolfram_code`); must match the knob.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bits: Option<u32>,
    /// Allowed options of a choice gene; a subset of the knob's options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<String>>,
    /// Per-gene mutation size, overriding the engine's `sigma`. `0` is
    /// allowed here (freezes this gene at its birth draw; see the module
    /// doc comment) even though the engine's own `sigma` must be positive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sigma: Option<f64>,
}

fn is_linear(s: &Scale) -> bool {
    *s == Scale::Linear
}

impl GeneSpec {
    /// A gene that inherits everything from the knob it names.
    pub fn new(key: impl Into<String>) -> Self {
        GeneSpec {
            key: key.into(),
            range: None,
            scale: Scale::Linear,
            bits: None,
            choices: None,
            sigma: None,
        }
    }

    /// A numeric gene over `[lo, hi]`.
    pub fn range(key: impl Into<String>, lo: f64, hi: f64) -> Self {
        GeneSpec {
            range: Some([lo, hi]),
            ..GeneSpec::new(key)
        }
    }

    /// A numeric gene over `[lo, hi]` sampled per decade.
    pub fn log_range(key: impl Into<String>, lo: f64, hi: f64) -> Self {
        GeneSpec {
            range: Some([lo, hi]),
            scale: Scale::Log,
            ..GeneSpec::new(key)
        }
    }
}

/// The shape of one gene once resolved against a grid.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneKind {
    Float { lo: f64, hi: f64, log: bool },
    Int { lo: i64, hi: i64 },
    Bool,
    Choice { options: Vec<String> },
    Bits { len: u32 },
}

/// One resolved gene.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gene {
    pub key: String,
    pub kind: GeneKind,
    /// Per-gene mutation size, if the spec gave one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sigma: Option<f64>,
}

impl Gene {
    /// A free gene over a numeric range (what a driver returns from
    /// `free_genes` as the default kind of a gene it understands).
    pub fn float(key: impl Into<String>, lo: f64, hi: f64, log: bool) -> Self {
        Gene {
            key: key.into(),
            kind: GeneKind::Float { lo, hi, log },
            sigma: None,
        }
    }
}

/// One value per gene, in the order of [`GeneSpace::genes`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Genome(pub Vec<ParamValue>);

/// Where a gene's value is written when a genome is applied to a grid.
#[derive(Clone, Debug, PartialEq)]
enum Target {
    /// One or more knob keys (more than one for a `[*]` wildcard).
    Params(Vec<String>),
    /// Nobody in the grid; a driver reads it.
    Free,
}

/// The genes of one search, resolved against a grid, plus the operators.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneSpace {
    genes: Vec<Gene>,
    targets: Vec<Target>,
    /// Genes a driver has claimed (`owned_keys`); `apply` skips them.
    owned: Vec<bool>,
}

fn invalid(key: &str, why: impl std::fmt::Display) -> ModelError {
    ModelError::InvalidParam(format!("gene '{key}': {why}"))
}

/// Expand a `rule.subrules[*].field` key into one key per subrule that has
/// the field; any other key comes back unchanged (as a one-element list).
fn expand_key(key: &str, descs: &[ParamDesc]) -> Vec<String> {
    let Some(rest) = key.strip_prefix("rule.subrules[*].") else {
        return vec![key.to_string()];
    };
    let mut out: Vec<String> = Vec::new();
    for d in descs {
        if let Some((i, field)) = parse_rule_key(&d.key)
            && field == rest
        {
            out.push(rule_key(i, field));
        }
    }
    out
}

fn is_prefixed(key: &str) -> bool {
    key.starts_with("rule.") || key.starts_with(MODEL_PREFIX)
}

fn kind_from_desc(spec: &GeneSpec, desc: &ParamDesc) -> Result<GeneKind, ModelError> {
    let key = &spec.key;
    let no_bits_or_choices = |what: &str| -> Result<(), ModelError> {
        if spec.bits.is_some() {
            return Err(invalid(
                key,
                format!("'bits' only applies to a bit-string knob, not {what}"),
            ));
        }
        if spec.choices.is_some() {
            return Err(invalid(
                key,
                format!("'choices' only applies to a choice knob, not {what}"),
            ));
        }
        Ok(())
    };
    match &desc.kind {
        ParamKind::Float { min, max, .. } => {
            no_bits_or_choices("a number")?;
            let [lo, hi] = spec.range.unwrap_or([*min, *max]);
            if lo > hi {
                return Err(invalid(key, format!("range [{lo}, {hi}] has lo > hi")));
            }
            if lo < *min || hi > *max {
                return Err(invalid(
                    key,
                    format!("range [{lo}, {hi}] is outside the knob's bounds [{min}, {max}]"),
                ));
            }
            let log = spec.scale == Scale::Log;
            if log && lo <= 0.0 {
                return Err(invalid(key, "log scale needs lo > 0"));
            }
            Ok(GeneKind::Float { lo, hi, log })
        }
        ParamKind::Int { min, max } => {
            no_bits_or_choices("a whole number")?;
            if spec.scale == Scale::Log {
                return Err(invalid(key, "log scale applies to decimal knobs only"));
            }
            let (lo, hi) = match spec.range {
                None => (*min, *max),
                Some([lo, hi]) => {
                    if lo.fract() != 0.0 || hi.fract() != 0.0 {
                        return Err(invalid(
                            key,
                            format!("range [{lo}, {hi}] must be whole numbers"),
                        ));
                    }
                    (lo as i64, hi as i64)
                }
            };
            if lo > hi {
                return Err(invalid(key, format!("range [{lo}, {hi}] has lo > hi")));
            }
            if lo < *min || hi > *max {
                return Err(invalid(
                    key,
                    format!("range [{lo}, {hi}] is outside the knob's bounds [{min}, {max}]"),
                ));
            }
            Ok(GeneKind::Int { lo, hi })
        }
        ParamKind::Bool => {
            no_bits_or_choices("an on/off switch")?;
            if spec.range.is_some() {
                return Err(invalid(key, "an on/off switch takes no range"));
            }
            Ok(GeneKind::Bool)
        }
        ParamKind::Choice { options } => {
            if spec.range.is_some() || spec.bits.is_some() {
                return Err(invalid(
                    key,
                    "a choice knob takes 'choices', not a range or bits",
                ));
            }
            let chosen = match &spec.choices {
                None => options.clone(),
                Some(c) => {
                    for name in c {
                        if !options.contains(name) {
                            return Err(invalid(
                                key,
                                format!("'{name}' is not one of: {}", options.join(", ")),
                            ));
                        }
                    }
                    c.clone()
                }
            };
            if chosen.is_empty() {
                return Err(invalid(key, "needs at least one choice"));
            }
            Ok(GeneKind::Choice { options: chosen })
        }
        ParamKind::Bits { len } => {
            if spec.range.is_some() || spec.choices.is_some() {
                return Err(invalid(
                    key,
                    "a bit-string knob takes 'bits', not a range or choices",
                ));
            }
            if let Some(b) = spec.bits
                && b != *len
            {
                return Err(invalid(
                    key,
                    format!("bits must be {len} for this knob, not {b}"),
                ));
            }
            Ok(GeneKind::Bits { len: *len })
        }
    }
}

fn kind_for_free(spec: &GeneSpec, defaults: &[Gene]) -> Result<GeneKind, ModelError> {
    let key = &spec.key;
    if let Some([lo, hi]) = spec.range {
        if spec.bits.is_some() || spec.choices.is_some() {
            return Err(invalid(key, "give one of range, bits or choices"));
        }
        if lo > hi {
            return Err(invalid(key, format!("range [{lo}, {hi}] has lo > hi")));
        }
        let log = spec.scale == Scale::Log;
        if log && lo <= 0.0 {
            return Err(invalid(key, "log scale needs lo > 0"));
        }
        return Ok(GeneKind::Float { lo, hi, log });
    }
    if let Some(len) = spec.bits {
        if spec.choices.is_some() {
            return Err(invalid(key, "give one of range, bits or choices"));
        }
        if !(1..=128).contains(&len) {
            return Err(invalid(key, "bits must be between 1 and 128"));
        }
        return Ok(GeneKind::Bits { len });
    }
    if let Some(options) = &spec.choices {
        if options.is_empty() {
            return Err(invalid(key, "needs at least one choice"));
        }
        return Ok(GeneKind::Choice {
            options: options.clone(),
        });
    }
    match defaults.iter().find(|g| g.key == *key) {
        Some(g) => Ok(g.kind.clone()),
        None if defaults.is_empty() => Err(invalid(
            key,
            "not a rule or model knob, and no driver is declared to interpret a free gene",
        )),
        None => {
            let known: Vec<&str> = defaults.iter().map(|g| g.key.as_str()).collect();
            Err(invalid(
                key,
                format!(
                    "the driver does not know this gene (known: {})",
                    known.join(", ")
                ),
            ))
        }
    }
}

impl GeneSpace {
    /// Resolve config genes against a grid.
    ///
    /// `free_defaults` are the genes a driver understands (with their default
    /// kinds) and `owned_keys` the knob keys the driver writes itself; pass
    /// empty slices when there is no driver. Errors say which gene is wrong
    /// and why, in words meant for the person who wrote the config.
    pub fn resolve(
        specs: &[GeneSpec],
        sim: &Sim,
        free_defaults: &[Gene],
        owned_keys: &[String],
    ) -> Result<Self, ModelError> {
        let descs = sim.params();
        let mut genes = Vec::with_capacity(specs.len());
        let mut targets = Vec::with_capacity(specs.len());
        let mut owned = Vec::with_capacity(specs.len());
        for spec in specs {
            let key = &spec.key;
            if genes.iter().any(|g: &Gene| g.key == *key) {
                return Err(invalid(key, "listed twice"));
            }
            if let Some(s) = spec.sigma
                && !(s >= 0.0 && s.is_finite())
            {
                return Err(invalid(key, "sigma must be a non-negative number"));
            }
            if is_prefixed(key) {
                let keys = expand_key(key, &descs);
                if keys.is_empty() {
                    return Err(invalid(key, "no subrule has that field"));
                }
                let mut kind: Option<GeneKind> = None;
                for k in &keys {
                    let desc = descs.iter().find(|d| d.key == *k).ok_or_else(|| {
                        let known: Vec<&str> = descs.iter().map(|d| d.key.as_str()).collect();
                        invalid(key, format!("unknown knob (known: {})", known.join(", ")))
                    })?;
                    if desc.read_only {
                        return Err(invalid(key, "that knob is read-only"));
                    }
                    let this = kind_from_desc(spec, desc)?;
                    match &kind {
                        None => kind = Some(this),
                        Some(k) if *k != this => {
                            return Err(invalid(
                                key,
                                "the subrules matched by [*] do not share one kind; list them separately",
                            ));
                        }
                        Some(_) => {}
                    }
                }
                let is_owned = owned_keys.iter().any(|o| keys.contains(o));
                genes.push(Gene {
                    key: key.clone(),
                    kind: kind.expect("at least one key"),
                    sigma: spec.sigma,
                });
                targets.push(Target::Params(keys));
                owned.push(is_owned);
            } else {
                let kind = kind_for_free(spec, free_defaults)?;
                genes.push(Gene {
                    key: key.clone(),
                    kind,
                    sigma: spec.sigma,
                });
                targets.push(Target::Free);
                owned.push(false);
            }
        }
        Ok(GeneSpace {
            genes,
            targets,
            owned,
        })
    }

    /// The resolved genes, in genome order.
    pub fn genes(&self) -> &[Gene] {
        &self.genes
    }

    /// Number of genes.
    pub fn len(&self) -> usize {
        self.genes.len()
    }

    /// True when no gene varies.
    pub fn is_empty(&self) -> bool {
        self.genes.is_empty()
    }

    /// Position of `key` in a genome.
    pub fn index(&self, key: &str) -> Option<usize> {
        self.genes.iter().position(|g| g.key == key)
    }

    /// Whether gene `i` is a free gene (read by a driver, not written to the grid).
    pub fn is_free(&self, i: usize) -> bool {
        matches!(self.targets.get(i), Some(Target::Free))
    }

    /// Draw a genome uniformly (log-uniformly for log genes) from the ranges.
    pub fn sample(&self, rng: &mut Rng) -> Genome {
        Genome(
            self.genes
                .iter()
                .map(|g| sample_kind(&g.kind, rng))
                .collect(),
        )
    }

    /// Read the grid's current values into a genome; free genes get the
    /// midpoint of their range (there is nowhere to read them from).
    pub fn from_sim(&self, sim: &Sim) -> Genome {
        Genome(
            self.genes
                .iter()
                .zip(&self.targets)
                .map(|(g, t)| match t {
                    Target::Params(keys) => {
                        sim.get_param(&keys[0]).unwrap_or_else(|| midpoint(&g.kind))
                    }
                    Target::Free => midpoint(&g.kind),
                })
                .collect(),
        )
    }

    /// Nudge every gene (see the module docs for the meaning of `sigma`).
    pub fn mutate(&self, rng: &mut Rng, genome: &mut Genome, sigma: f64) {
        for (g, v) in self.genes.iter().zip(genome.0.iter_mut()) {
            let s = g.sigma.unwrap_or(sigma);
            mutate_value(&g.kind, v, s, rng);
        }
    }

    /// Nudge a single gene by position; used by the GUI's "mutate rule".
    pub fn mutate_one(&self, rng: &mut Rng, genome: &mut Genome, i: usize, sigma: f64) {
        if let (Some(g), Some(v)) = (self.genes.get(i), genome.0.get_mut(i)) {
            mutate_value(&g.kind, v, g.sigma.unwrap_or(sigma), rng);
        }
    }

    /// Uniform crossover: each gene comes from parent `a` or `b` with equal
    /// chance; a bit-string gene is cut at one random point instead.
    pub fn crossover(&self, rng: &mut Rng, a: &Genome, b: &Genome) -> Genome {
        Genome(
            self.genes
                .iter()
                .zip(a.0.iter().zip(&b.0))
                .map(|(g, (x, y))| match (&g.kind, x, y) {
                    (GeneKind::Bits { len }, ParamValue::Bits(p), ParamValue::Bits(q)) => {
                        let cut = rng.below(*len as usize + 1) as u32;
                        let low_mask = if cut >= 128 {
                            u128::MAX
                        } else {
                            (1u128 << cut) - 1
                        };
                        ParamValue::Bits((p & low_mask) | (q & !low_mask))
                    }
                    _ => {
                        if rng.uniform() < 0.5 {
                            x.clone()
                        } else {
                            y.clone()
                        }
                    }
                })
                .collect(),
        )
    }

    /// Pull every gene back inside its range (numbers), options (choices) or
    /// width (bits). Values of the wrong shape are replaced by the midpoint.
    pub fn clamp(&self, genome: &mut Genome) {
        for (g, v) in self.genes.iter().zip(genome.0.iter_mut()) {
            *v = clamp_value(&g.kind, v);
        }
    }

    /// Write a genome into a grid: every rule/model gene that no driver owns
    /// is set through [`Sim::set_param`]. Stops at the first refusal, which
    /// is returned; the caller decides whether that genome is "invalid".
    pub fn apply(&self, sim: &mut Sim, genome: &Genome) -> Result<(), ModelError> {
        if genome.0.len() != self.genes.len() {
            return Err(ModelError::InvalidParam(format!(
                "genome has {} values for {} genes",
                genome.0.len(),
                self.genes.len()
            )));
        }
        for ((t, owned), v) in self.targets.iter().zip(&self.owned).zip(&genome.0) {
            if *owned {
                continue;
            }
            if let Target::Params(keys) = t {
                for k in keys {
                    sim.set_param(k, v.clone())?;
                }
            }
        }
        Ok(())
    }

    /// The genome as `key -> value`, for reports.
    pub fn named(&self, genome: &Genome) -> BTreeMap<String, ParamValue> {
        self.genes
            .iter()
            .zip(&genome.0)
            .map(|(g, v)| (g.key.clone(), v.clone()))
            .collect()
    }

    /// One gene's value.
    pub fn get<'a>(&self, genome: &'a Genome, key: &str) -> Option<&'a ParamValue> {
        genome.0.get(self.index(key)?)
    }

    /// One gene's value as a number (`Int` widened, `Bool` as 0/1); `None`
    /// for a missing key or a choice/bits gene.
    pub fn float(&self, genome: &Genome, key: &str) -> Option<f64> {
        match self.get(genome, key)? {
            ParamValue::Float(v) => Some(*v),
            ParamValue::Int(v) => Some(*v as f64),
            ParamValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            ParamValue::Choice(_) | ParamValue::Bits(_) => None,
        }
    }
}

fn midpoint(kind: &GeneKind) -> ParamValue {
    match kind {
        GeneKind::Float { lo, hi, log } => ParamValue::Float(if *log {
            (lo.ln() + (hi.ln() - lo.ln()) / 2.0).exp()
        } else {
            (lo + hi) / 2.0
        }),
        GeneKind::Int { lo, hi } => ParamValue::Int((lo + hi) / 2),
        GeneKind::Bool => ParamValue::Bool(false),
        GeneKind::Choice { options } => ParamValue::Choice(options[0].clone()),
        GeneKind::Bits { .. } => ParamValue::Bits(0),
    }
}

fn bits_mask(len: u32) -> u128 {
    if len >= 128 {
        u128::MAX
    } else {
        (1u128 << len) - 1
    }
}

fn sample_kind(kind: &GeneKind, rng: &mut Rng) -> ParamValue {
    match kind {
        GeneKind::Float { lo, hi, log } => ParamValue::Float(if *log {
            rng.log_uniform(*lo, *hi)
        } else {
            lo + rng.uniform() * (hi - lo)
        }),
        GeneKind::Int { lo, hi } => {
            let span = (hi - lo + 1) as usize;
            ParamValue::Int(lo + rng.below(span) as i64)
        }
        GeneKind::Bool => ParamValue::Bool(rng.uniform() < 0.5),
        GeneKind::Choice { options } => {
            ParamValue::Choice(options[rng.below(options.len())].clone())
        }
        GeneKind::Bits { len } => {
            let v = ((rng.next_u64() as u128) << 64) | rng.next_u64() as u128;
            ParamValue::Bits(v & bits_mask(*len))
        }
    }
}

fn mutate_value(kind: &GeneKind, v: &mut ParamValue, sigma: f64, rng: &mut Rng) {
    match (kind, &*v) {
        (GeneKind::Float { lo, hi, log }, ParamValue::Float(x)) => {
            let y = if *log {
                x * (sigma * rng.normal()).exp()
            } else {
                x + sigma * (hi - lo) * rng.normal()
            };
            *v = ParamValue::Float(y.clamp(*lo, *hi));
        }
        (GeneKind::Int { lo, hi }, ParamValue::Int(x)) => {
            let step = (sigma * (*hi - *lo) as f64 * rng.normal()).round() as i64;
            *v = ParamValue::Int((x + step).clamp(*lo, *hi));
        }
        (GeneKind::Bool, ParamValue::Bool(b)) => {
            if rng.uniform() < sigma {
                *v = ParamValue::Bool(!b);
            }
        }
        (GeneKind::Choice { options }, ParamValue::Choice(_)) => {
            if rng.uniform() < sigma {
                *v = ParamValue::Choice(options[rng.below(options.len())].clone());
            }
        }
        (GeneKind::Bits { len }, ParamValue::Bits(x)) => {
            let p = (8.0 * sigma / f64::from(*len)).min(1.0);
            let mut y = *x;
            for bit in 0..*len {
                if rng.uniform() < p {
                    y ^= 1u128 << bit;
                }
            }
            *v = ParamValue::Bits(y & bits_mask(*len));
        }
        // A value of the wrong shape: replace it with something legal.
        _ => *v = midpoint(kind),
    }
}

fn clamp_value(kind: &GeneKind, v: &ParamValue) -> ParamValue {
    match (kind, v) {
        (GeneKind::Float { lo, hi, .. }, ParamValue::Float(x)) => {
            ParamValue::Float(x.clamp(*lo, *hi))
        }
        (GeneKind::Int { lo, hi }, ParamValue::Int(x)) => ParamValue::Int(*x.clamp(lo, hi)),
        (GeneKind::Bool, ParamValue::Bool(b)) => ParamValue::Bool(*b),
        (GeneKind::Choice { options }, ParamValue::Choice(c)) if options.contains(c) => {
            ParamValue::Choice(c.clone())
        }
        (GeneKind::Bits { len }, ParamValue::Bits(x)) => ParamValue::Bits(x & bits_mask(*len)),
        _ => midpoint(kind),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid1d::Grid1D;
    use crate::grid2d::Grid2D;
    use crate::rules::{CountOp, Neighborhood2D, Rule1D, Rule1DSubrule, Rule2D, Rule2DSubrule};
    use crate::types::CellType;

    fn life() -> Sim {
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
        let rule = Rule2D {
            subrules: vec![
                sub(alive, 4, CountOp::Gt, None, dead),
                sub(alive, 2, CountOp::Gt, Some(3), alive),
                sub(dead, 3, CountOp::Eq, None, alive),
            ],
        };
        Sim::D2(Grid2D::new(4, 4, 0, vec![dead; 16], rule))
    }

    fn rule30() -> Sim {
        let x = CellType::from("X");
        let mk = |cur| Rule1DSubrule {
            current_type: cur,
            criteria_type: x,
            wolfram_code: 30,
            n: 1,
            randomness: None,
            output_type: x,
        };
        let rule = Rule1D {
            subrules: vec![mk(x), mk(CellType::inactive())],
        };
        Sim::D1(Grid1D::new(9, 0, vec![CellType::inactive(); 9], rule))
    }

    fn spec(json: &str) -> GeneSpec {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn specs_parse_the_flat_documented_shapes() {
        let s = spec(r#"{"key": "model.p0", "range": [0.08, 0.6], "scale": "log"}"#);
        assert_eq!(s, GeneSpec::log_range("model.p0", 0.08, 0.6));
        let s = spec(r#"{"key": "rule.subrules[*].wolfram_code", "bits": 8, "sigma": 0.01}"#);
        assert_eq!(s.bits, Some(8));
        assert_eq!(s.sigma, Some(0.01));
        let s = spec(r#"{"key": "rule.subrules[2].op", "choices": ["eq", "gt"]}"#);
        assert_eq!(
            s.choices.as_deref(),
            Some(&["eq".to_string(), "gt".to_string()][..])
        );
        assert!(serde_json::from_str::<GeneSpec>(r#"{"key": "x", "bogus": 1}"#).is_err());
        let back: GeneSpec =
            serde_json::from_str(&serde_json::to_string(&GeneSpec::new("k")).unwrap()).unwrap();
        assert_eq!(back, GeneSpec::new("k"));
    }

    #[test]
    fn prefixed_genes_inherit_narrow_and_expand_wildcards() {
        let sim = life();
        let specs = vec![
            GeneSpec::new("rule.subrules[0].count"),
            GeneSpec::range("rule.subrules[1].limit", 2.0, 8.0),
            spec(r#"{"key": "rule.subrules[2].op", "choices": ["eq", "gt"]}"#),
            GeneSpec::new("rule.subrules[*].range"),
        ];
        let space = GeneSpace::resolve(&specs, &sim, &[], &[]).unwrap();
        assert_eq!(space.len(), 4);
        assert_eq!(space.genes()[0].kind, GeneKind::Int { lo: 0, hi: 8 });
        assert_eq!(space.genes()[1].kind, GeneKind::Int { lo: 2, hi: 8 });
        assert_eq!(
            space.genes()[2].kind,
            GeneKind::Choice {
                options: vec!["eq".into(), "gt".into()]
            }
        );
        assert_eq!(space.genes()[3].kind, GeneKind::Int { lo: 1, hi: 8 });
        assert_eq!(
            space.targets[3],
            Target::Params(vec![
                "rule.subrules[0].range".into(),
                "rule.subrules[1].range".into(),
                "rule.subrules[2].range".into()
            ])
        );
        assert!(!space.is_free(0));
        assert_eq!(space.index("rule.subrules[*].range"), Some(3));

        let r = rule30();
        let space = GeneSpace::resolve(
            &[spec(
                r#"{"key": "rule.subrules[*].wolfram_code", "bits": 8}"#,
            )],
            &r,
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(space.genes()[0].kind, GeneKind::Bits { len: 8 });
        let space = GeneSpace::resolve(
            &[GeneSpec::new("rule.subrules[0].wolfram_code")],
            &r,
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(space.genes()[0].kind, GeneKind::Bits { len: 8 });
    }

    #[test]
    fn free_genes_come_from_the_spec_or_the_driver() {
        let sim = life();
        let defaults = [
            Gene::float("tau_days", 2.0, 100.0, true),
            Gene::float("wind_scale", 0.0, 1.5, false),
        ];
        let specs = vec![
            GeneSpec::range("wind_scale", 0.0, 1.0),
            GeneSpec::new("tau_days"),
            spec(r#"{"key": "mode", "choices": ["a", "b"]}"#),
            spec(r#"{"key": "flags", "bits": 4}"#),
        ];
        let space = GeneSpace::resolve(&specs, &sim, &defaults, &[]).unwrap();
        assert_eq!(
            space.genes()[0].kind,
            GeneKind::Float {
                lo: 0.0,
                hi: 1.0,
                log: false
            }
        );
        assert_eq!(
            space.genes()[1].kind,
            GeneKind::Float {
                lo: 2.0,
                hi: 100.0,
                log: true
            }
        );
        assert_eq!(
            space.genes()[2].kind,
            GeneKind::Choice {
                options: vec!["a".into(), "b".into()]
            }
        );
        assert_eq!(space.genes()[3].kind, GeneKind::Bits { len: 4 });
        assert!(space.is_free(0) && space.is_free(3));
    }

    #[test]
    fn every_documented_refusal_names_the_gene() {
        let sim = life();
        let r = rule30();
        let cases: Vec<(&Sim, GeneSpec, &str)> = vec![
            (
                &sim,
                GeneSpec::new("rule.subrules[9].count"),
                "unknown knob",
            ),
            (&sim, GeneSpec::new("model.p0"), "unknown knob"),
            (
                &sim,
                GeneSpec::range("rule.subrules[0].count", 0.0, 9.0),
                "outside the knob's bounds",
            ),
            (
                &sim,
                GeneSpec::range("rule.subrules[0].count", 5.0, 2.0),
                "lo > hi",
            ),
            (
                &sim,
                GeneSpec::range("rule.subrules[0].count", 0.5, 2.0),
                "whole numbers",
            ),
            (
                &sim,
                spec(r#"{"key": "rule.subrules[0].count", "bits": 8}"#),
                "'bits' only applies",
            ),
            (
                &sim,
                spec(r#"{"key": "rule.subrules[0].count", "choices": ["1"]}"#),
                "'choices' only applies",
            ),
            (
                &sim,
                spec(r#"{"key": "rule.subrules[0].count", "scale": "log"}"#),
                "decimal knobs only",
            ),
            (
                &sim,
                spec(r#"{"key": "rule.subrules[0].op", "choices": ["ge"]}"#),
                "is not one of",
            ),
            (
                &sim,
                spec(r#"{"key": "rule.subrules[0].op", "range": [0, 1]}"#),
                "takes 'choices'",
            ),
            (
                &sim,
                spec(r#"{"key": "rule.subrules[0].op", "choices": []}"#),
                "at least one choice",
            ),
            (
                &sim,
                spec(r#"{"key": "rule.subrules[*].nothing"}"#),
                "no subrule has that field",
            ),
            (&sim, GeneSpec::new("wind_scale"), "no driver is declared"),
            (&sim, GeneSpec::range("free", 3.0, 1.0), "lo > hi"),
            (
                &sim,
                GeneSpec::log_range("free", 0.0, 1.0),
                "log scale needs lo > 0",
            ),
            (
                &sim,
                spec(r#"{"key": "free", "range": [0, 1], "bits": 3}"#),
                "one of range, bits or choices",
            ),
            (
                &sim,
                spec(r#"{"key": "free", "bits": 300}"#),
                "between 1 and 128",
            ),
            (
                &sim,
                spec(r#"{"key": "free", "choices": []}"#),
                "at least one choice",
            ),
            (
                &sim,
                spec(r#"{"key": "rule.subrules[0].count", "sigma": -1}"#),
                "sigma must be",
            ),
            (
                &r,
                spec(r#"{"key": "rule.subrules[0].wolfram_code", "bits": 32}"#),
                "bits must be 8",
            ),
            (
                &r,
                spec(r#"{"key": "rule.subrules[0].wolfram_code", "range": [0, 1]}"#),
                "takes 'bits'",
            ),
        ];
        for (s, g, expect) in cases {
            let err = GeneSpace::resolve(std::slice::from_ref(&g), s, &[], &[]).unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains(&g.key) && msg.contains(expect),
                "{msg} (wanted {expect})"
            );
        }
        // Duplicates, driver-unknown free genes, read-only knobs.
        let dup = [
            GeneSpec::new("rule.subrules[0].count"),
            GeneSpec::new("rule.subrules[0].count"),
        ];
        assert!(
            format!("{}", GeneSpace::resolve(&dup, &sim, &[], &[]).unwrap_err())
                .contains("listed twice")
        );
        let defaults = [Gene::float("tau_days", 2.0, 100.0, true)];
        let err =
            GeneSpace::resolve(&[GeneSpec::new("wind_scal")], &sim, &defaults, &[]).unwrap_err();
        assert!(format!("{err}").contains("known: tau_days"), "{err}");
        // A read-only knob (the wildfire model's seed) cannot be a gene.
        let forest = CellType::from("Forest");
        let mut wf = Grid2D::new(4, 4, 0, vec![forest; 16], Rule2D { subrules: vec![] });
        wf.attach_model(Box::new(crate::wildfire::WildfireModel::new(
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
        )))
        .unwrap();
        let wf = Sim::D2(wf);
        let err = GeneSpace::resolve(&[GeneSpec::new("model.seed")], &wf, &[], &[]).unwrap_err();
        assert!(format!("{err}").contains("read-only"), "{err}");
        let ok = GeneSpace::resolve(&[GeneSpec::log_range("model.p0", 0.05, 0.6)], &wf, &[], &[])
            .unwrap();
        assert_eq!(
            ok.genes()[0].kind,
            GeneKind::Float {
                lo: 0.05,
                hi: 0.6,
                log: true
            }
        );
        // A [*] whose subrules disagree on kind: count bounds differ by neighbourhood.
        let mut mixed = life();
        mixed
            .set_param(
                "rule.subrules[0].neighborhood",
                ParamValue::Choice("VonNeumann".into()),
            )
            .unwrap();
        let err = GeneSpace::resolve(&[GeneSpec::new("rule.subrules[*].count")], &mixed, &[], &[])
            .unwrap_err();
        assert!(format!("{err}").contains("do not share one kind"), "{err}");
    }

    #[test]
    fn sampling_and_mutation_stay_inside_every_kind_of_range() {
        let sim = life();
        let r = rule30();
        let specs = vec![
            GeneSpec::range("rule.subrules[0].count", 1.0, 6.0),
            GeneSpec::log_range("p", 0.01, 10.0),
            GeneSpec::range("q", -2.0, 2.0),
            spec(r#"{"key": "rule.subrules[2].op", "choices": ["eq", "gt"]}"#),
            spec(r#"{"key": "flag", "choices": ["on", "off"]}"#),
        ];
        let space = GeneSpace::resolve(&specs, &sim, &[], &[]).unwrap();
        let bits_space = GeneSpace::resolve(
            &[GeneSpec::new("rule.subrules[0].wolfram_code")],
            &r,
            &[],
            &[],
        )
        .unwrap();
        let mut rng = Rng::new(3);
        let inside = |g: &Genome| {
            matches!(g.0[0], ParamValue::Int(c) if (1..=6).contains(&c))
                && matches!(g.0[1], ParamValue::Float(p) if (0.01..=10.0).contains(&p))
                && matches!(g.0[2], ParamValue::Float(q) if (-2.0..=2.0).contains(&q))
                && matches!(&g.0[3], ParamValue::Choice(c) if c == "eq" || c == "gt")
                && matches!(&g.0[4], ParamValue::Choice(c) if c == "on" || c == "off")
        };
        let mut log_small = 0;
        for _ in 0..1000 {
            let mut g = space.sample(&mut rng);
            assert!(inside(&g), "{g:?}");
            if let ParamValue::Float(p) = g.0[1]
                && p < 0.1
            {
                log_small += 1;
            }
            space.mutate(&mut rng, &mut g, 0.2);
            assert!(inside(&g), "after mutation {g:?}");
            let b = bits_space.sample(&mut rng);
            assert!(matches!(b.0[0], ParamValue::Bits(v) if v < 256));
        }
        // Log-uniform on [0.01, 10]: a third of the mass is below 0.1.
        assert!(
            (250..=420).contains(&log_small),
            "log sampling: {log_small}"
        );
        // Log mutation is multiplicative: a small value moves a little.
        let mut g = Genome(vec![
            ParamValue::Int(3),
            ParamValue::Float(0.02),
            ParamValue::Float(0.0),
            ParamValue::Choice("eq".into()),
            ParamValue::Choice("on".into()),
        ]);
        let mut moved = Vec::new();
        for _ in 0..200 {
            let mut h = g.clone();
            space.mutate(&mut rng, &mut h, 0.1);
            if let ParamValue::Float(p) = h.0[1] {
                moved.push((p / 0.02).ln().abs());
            }
        }
        let mean_move = moved.iter().sum::<f64>() / moved.len() as f64;
        assert!(
            mean_move < 0.2,
            "relative moves of about sigma: {mean_move}"
        );
        // Bits: about 8*sigma flips per mutation.
        let mut flips = 0u32;
        for _ in 0..500 {
            let mut b = Genome(vec![ParamValue::Bits(0)]);
            bits_space.mutate(&mut rng, &mut b, 0.2);
            if let ParamValue::Bits(v) = b.0[0] {
                flips += v.count_ones();
            }
        }
        let per = f64::from(flips) / 500.0;
        assert!((1.0..2.4).contains(&per), "flips per mutation {per}");
        // mutate_one touches only the chosen gene.
        space.mutate_one(&mut rng, &mut g, 0, 0.5);
        assert_eq!(g.0[1], ParamValue::Float(0.02));
        assert!(inside(&g));
        // clamp pulls stray values back and repairs wrong shapes.
        let mut wild = Genome(vec![
            ParamValue::Int(99),
            ParamValue::Float(1e9),
            ParamValue::Bool(true),
            ParamValue::Choice("lt".into()),
            ParamValue::Choice("off".into()),
        ]);
        space.clamp(&mut wild);
        assert_eq!(wild.0[0], ParamValue::Int(6));
        assert_eq!(wild.0[1], ParamValue::Float(10.0));
        assert!(
            matches!(wild.0[2], ParamValue::Float(_)),
            "wrong shape replaced"
        );
        assert_eq!(
            wild.0[3],
            ParamValue::Choice("eq".into()),
            "not an allowed choice: first option"
        );
        assert_eq!(wild.0[4], ParamValue::Choice("off".into()));
    }

    /// Round 7 Task 5 (E45's `SMC_WIND_ROT_SIGMA=0`): a per-gene `sigma` of
    /// exactly `0.0` must resolve (not be rejected the way a negative
    /// sigma is) and must freeze that one gene under repeated mutation --
    /// every member keeps exactly the value it was born with -- while a
    /// second gene with no override in the same space keeps mutating
    /// normally at the engine's own sigma. This is the library-level half
    /// of the "does the library reject sigma 0" question E45's brief asks;
    /// it does not (this test is the acceptance check for that fix).
    #[test]
    fn a_per_gene_sigma_of_zero_is_accepted_and_freezes_that_gene_only() {
        let sim = life();
        let frozen = spec(r#"{"key": "q", "range": [-2.0, 2.0], "sigma": 0.0}"#);
        assert_eq!(frozen.sigma, Some(0.0));
        let specs = vec![
            frozen,
            GeneSpec::range("rule.subrules[0].count", 1.0, 6.0),
        ];
        let space = GeneSpace::resolve(&specs, &sim, &[], &[])
            .expect("sigma: 0.0 must resolve, unlike a negative sigma");
        let mut rng = Rng::new(11);
        let mut g = Genome(vec![ParamValue::Float(0.75), ParamValue::Int(3)]);
        let born = g.clone();
        for _ in 0..200 {
            // Engine sigma 0.5 would move an unfrozen float gene a lot over
            // 200 mutations; the frozen gene (index 0) must never move.
            space.mutate(&mut rng, &mut g, 0.5);
            assert_eq!(
                g.0[0], born.0[0],
                "sigma: 0.0 must be a no-op on the gene it overrides"
            );
        }
        // The other gene (no per-gene override) did mutate at the engine's
        // sigma -- confirms the freeze is per-gene, not a global accident.
        assert_ne!(
            g.0[1], born.0[1],
            "the non-overridden gene should have moved under 200 mutations at sigma 0.5"
        );
    }

    #[test]
    fn crossover_takes_each_gene_from_a_parent_and_cuts_bit_strings_once() {
        let r = rule30();
        let specs = vec![
            GeneSpec::new("rule.subrules[0].wolfram_code"),
            GeneSpec::range("x", 0.0, 1.0),
        ];
        let space = GeneSpace::resolve(&specs, &r, &[], &[]).unwrap();
        let a = Genome(vec![ParamValue::Bits(0b1111_0000), ParamValue::Float(0.0)]);
        let b = Genome(vec![ParamValue::Bits(0b0000_1111), ParamValue::Float(1.0)]);
        let mut rng = Rng::new(11);
        let mut seen_a = false;
        let mut seen_b = false;
        for _ in 0..200 {
            let c = space.crossover(&mut rng, &a, &b);
            match &c.0[1] {
                ParamValue::Float(v) if *v == 0.0 => seen_a = true,
                ParamValue::Float(v) if *v == 1.0 => seen_b = true,
                other => panic!("child gene from nowhere: {other:?}"),
            }
            if let ParamValue::Bits(v) = c.0[0] {
                // Low bits from a, high bits from b, for some cut point.
                assert!(
                    (0..=8).any(|cut| {
                        let m = if cut >= 8 { 0xFF } else { (1u128 << cut) - 1 };
                        v == ((0b1111_0000u128 & m) | (0b0000_1111u128 & !m & 0xFF))
                    }),
                    "{v:#b}"
                );
            }
        }
        assert!(seen_a && seen_b);
        let mut r1 = Rng::new(5);
        let mut r2 = Rng::new(5);
        assert_eq!(
            space.crossover(&mut r1, &a, &b),
            space.crossover(&mut r2, &a, &b),
            "deterministic"
        );
    }

    #[test]
    fn apply_writes_knobs_skips_free_and_owned_genes_and_reports_refusals() {
        let mut sim = life();
        let defaults = [Gene::float("wind_scale", 0.0, 1.5, false)];
        let specs = vec![
            GeneSpec::new("rule.subrules[0].count"),
            GeneSpec::new("rule.subrules[*].range"),
            GeneSpec::new("wind_scale"),
            GeneSpec::new("rule.subrules[1].limit"),
        ];
        let owned = vec!["rule.subrules[1].limit".to_string()];
        let space = GeneSpace::resolve(&specs, &sim, &defaults, &owned).unwrap();
        let g = Genome(vec![
            ParamValue::Int(5),
            ParamValue::Int(2),
            ParamValue::Float(0.7),
            ParamValue::Int(7),
        ]);
        space.apply(&mut sim, &g).unwrap();
        assert_eq!(
            sim.get_param("rule.subrules[0].count"),
            Some(ParamValue::Int(5))
        );
        for i in 0..3 {
            assert_eq!(
                sim.get_param(&rule_key(i, "range")),
                Some(ParamValue::Int(2))
            );
        }
        assert_eq!(
            sim.get_param("rule.subrules[1].limit"),
            Some(ParamValue::Int(3)),
            "owned gene left to the driver"
        );
        let named = space.named(&g);
        assert_eq!(named["wind_scale"], ParamValue::Float(0.7));
        assert_eq!(space.float(&g, "wind_scale"), Some(0.7));
        assert_eq!(space.float(&g, "rule.subrules[0].count"), Some(5.0));
        assert_eq!(space.float(&g, "nope"), None);
        assert_eq!(
            space.get(&g, "rule.subrules[*].range"),
            Some(&ParamValue::Int(2))
        );
        // Reading back: free genes get their midpoint.
        let read = space.from_sim(&sim);
        assert_eq!(read.0[0], ParamValue::Int(5));
        assert_eq!(read.0[2], ParamValue::Float(0.75));
        // A value the rule engine refuses (a gt limit below its count) comes
        // back as the error, and the grid keeps its old value.
        let mut sim2 = life();
        let space2 =
            GeneSpace::resolve(&[GeneSpec::new("rule.subrules[1].limit")], &sim2, &[], &[])
                .unwrap();
        assert!(
            space2
                .apply(&mut sim2, &Genome(vec![ParamValue::Int(1)]))
                .is_err()
        );
        assert_eq!(
            sim2.get_param("rule.subrules[1].limit"),
            Some(ParamValue::Int(3))
        );
        assert!(
            space2.apply(&mut sim2, &Genome(vec![])).is_err(),
            "length mismatch"
        );
        assert_eq!(
            space2.float(
                &Genome(vec![ParamValue::Choice("x".into())]),
                "rule.subrules[1].limit"
            ),
            None
        );
    }

    /// `rule.subrules[i].randomness` is the only built-in Float knob, so it
    /// is the only way to reach `kind_from_desc`'s own Float-branch
    /// refusals (the Int branch's copies of the same three checks are
    /// covered above via `rule.subrules[0].count`).
    #[test]
    fn float_knob_refusals_and_a_choice_knobs_default_options() {
        let mut sim = life();
        // `randomness` only shows up in the knob list once a subrule has a
        // value for it (the module doc's "a write can turn randomness on");
        // life()'s own fixture never sets it, so give the first subrule one.
        sim.set_param("rule.subrules[0].randomness", ParamValue::Float(0.2))
            .unwrap();
        let bad_order = GeneSpace::resolve(
            &[GeneSpec::range("rule.subrules[0].randomness", 0.9, 0.1)],
            &sim,
            &[],
            &[],
        )
        .unwrap_err();
        assert!(format!("{bad_order}").contains("lo > hi"), "{bad_order}");

        let out_of_bounds = GeneSpace::resolve(
            &[GeneSpec::range("rule.subrules[0].randomness", -0.1, 0.5)],
            &sim,
            &[],
            &[],
        )
        .unwrap_err();
        assert!(
            format!("{out_of_bounds}").contains("outside the knob's bounds"),
            "{out_of_bounds}"
        );

        let bad_log = GeneSpace::resolve(
            &[spec(
                r#"{"key": "rule.subrules[0].randomness", "range": [0.0, 0.5], "scale": "log"}"#,
            )],
            &sim,
            &[],
            &[],
        )
        .unwrap_err();
        assert!(
            format!("{bad_log}").contains("log scale needs lo > 0"),
            "{bad_log}"
        );

        // A choice knob with no explicit "choices" takes every option the
        // knob itself offers (as opposed to the narrowed-subset case the
        // refusal table above already covers).
        let all_ops =
            GeneSpace::resolve(&[GeneSpec::new("rule.subrules[0].op")], &sim, &[], &[]).unwrap();
        assert_eq!(
            all_ops.genes()[0].kind,
            GeneKind::Choice {
                options: vec!["lt".into(), "gt".into(), "eq".into()]
            }
        );

        // A free gene naming both `bits` and `choices` is as ambiguous as
        // naming both `range` and `bits` (already covered above).
        let both = GeneSpace::resolve(
            &[spec(r#"{"key": "free3", "bits": 4, "choices": ["a", "b"]}"#)],
            &sim,
            &[],
            &[],
        )
        .unwrap_err();
        assert!(
            format!("{both}").contains("one of range, bits or choices"),
            "{both}"
        );
    }

    /// `GeneKind::Bool` has no built-in knob in this crate (only an
    /// out-of-tree `ExternalModel` test fixture in `tests/external_model.rs`
    /// declares one), so its four value operators -- otherwise identical in
    /// shape to every other kind's, already covered above -- are exercised
    /// directly here instead of through `GeneSpace::resolve`.
    #[test]
    fn bool_gene_values_sample_mutate_clamp_and_widen_to_a_number() {
        let kind = GeneKind::Bool;
        assert_eq!(midpoint(&kind), ParamValue::Bool(false));
        assert_eq!(
            clamp_value(&kind, &ParamValue::Bool(true)),
            ParamValue::Bool(true)
        );
        // A value of the wrong shape falls back to the midpoint, same rule
        // every other kind's clamp/mutate already follows.
        assert_eq!(clamp_value(&kind, &ParamValue::Int(1)), ParamValue::Bool(false));

        let mut rng = Rng::new(3);
        let (mut saw_true, mut saw_false) = (false, false);
        for _ in 0..50 {
            match sample_kind(&kind, &mut rng) {
                ParamValue::Bool(true) => saw_true = true,
                ParamValue::Bool(false) => saw_false = true,
                other => panic!("unexpected {other:?}"),
            }
        }
        assert!(saw_true && saw_false, "sample_kind should draw both");

        let mut v = ParamValue::Bool(false);
        let mut flipped = false;
        for _ in 0..20 {
            mutate_value(&kind, &mut v, 1.0, &mut rng);
            if v == ParamValue::Bool(true) {
                flipped = true;
            }
        }
        assert!(flipped, "sigma 1.0 should flip a bool gene");
        let mut wrong_shape = ParamValue::Int(7);
        mutate_value(&kind, &mut wrong_shape, 1.0, &mut rng);
        assert_eq!(
            wrong_shape,
            ParamValue::Bool(false),
            "a wrong-shape value is replaced by the midpoint, not mutated"
        );

        // `GeneSpace::float` widens a Bool gene's value to 0.0/1.0, the same
        // as it widens Int -- built by hand since no built-in knob is Bool.
        let space = GeneSpace {
            genes: vec![Gene {
                key: "flag".into(),
                kind,
                sigma: None,
            }],
            targets: vec![Target::Free],
            owned: vec![false],
        };
        assert_eq!(
            space.float(&Genome(vec![ParamValue::Bool(true)]), "flag"),
            Some(1.0)
        );
    }

    /// `midpoint`'s Float-log, Int, and Bits branches are each exercised
    /// elsewhere in an integration path already; this closes the direct,
    /// per-kind check the way the Bool test above does for its own kind.
    #[test]
    fn midpoint_covers_every_kind_directly() {
        match midpoint(&GeneKind::Float {
            lo: 1.0,
            hi: 100.0,
            log: true,
        }) {
            ParamValue::Float(v) => assert!(
                (v - 10.0).abs() < 1e-9,
                "log midpoint of [1, 100] is 10 (got {v})"
            ),
            other => panic!("expected a float, got {other:?}"),
        }
        assert_eq!(
            midpoint(&GeneKind::Int { lo: 2, hi: 8 }),
            ParamValue::Int(5)
        );
        assert_eq!(midpoint(&GeneKind::Bits { len: 4 }), ParamValue::Bits(0));
    }
}
