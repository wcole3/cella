//! Archives for illumination: a grid of "the best I found that behaves like
//! *this*", one cell per region of behaviour space.
//!
//! An [`Archive`] is the heart of MAP-Elites (Mouret & Clune 2015). Pick one
//! to three **descriptors** — plain [`Metric`]s such as activity and entropy —
//! and split each one's range into bins. Every evaluated genome (one set of
//! knob values; see [`super::genome`]) lands in one cell of that grid
//! according to how it behaved; the cell keeps only the best-scoring genome
//! it has ever seen (its **elite**). Ties go to the incumbent. Filling the grid is
//! the goal: a full archive is a map of everything the rule family can do,
//! with the best example of each behaviour ready to load.
//!
//! Vocabulary (the same names the pyribs library uses, so papers read the
//! same): **coverage** is the share of cells that hold an elite; **QD score**
//! is the sum of elite fitnesses (with an offset so negative scores still add
//! up); `obj_max` / `obj_mean` are the best and mean elite fitness.
//!
//! Each elite carries a small [`Thumbnail`] of its final grid so a gallery
//! can show what it looks like without re-running it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::metrics::{Metric, When};
use super::sim::Sim;
use crate::external::{ModelError, ParamValue};
use crate::rng::Rng;
use crate::types::CellType;

/// One archive axis as written in a config.
///
/// ```json
/// {"metric": "activity", "when": "mean", "bins": 20}
/// {"metric": "entropy", "range": [0.0, 1.0]}
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DescriptorSpec {
    /// What to measure along this axis (must be a descriptor metric).
    #[serde(flatten)]
    pub metric: Metric,
    /// Which reading of the run counts (see [`When`]).
    #[serde(default)]
    pub when: When,
    /// Extent of the axis; defaults to the metric's natural range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<[f64; 2]>,
    /// How many cells along this axis.
    #[serde(default = "default_bins")]
    pub bins: u32,
}

fn default_bins() -> u32 {
    20
}

impl DescriptorSpec {
    /// An axis over `metric` with the default range and bin count.
    pub fn new(metric: Metric) -> Self {
        DescriptorSpec {
            metric,
            when: When::End,
            range: None,
            bins: default_bins(),
        }
    }
}

/// The resolved axes of an archive: the specs with every default filled in.
/// The four vectors all have one entry per axis.
#[derive(Clone, Debug, PartialEq)]
pub struct Descriptor {
    /// The axes as configured.
    pub specs: Vec<DescriptorSpec>,
    /// `[low, high]` of each axis (the spec's own, or the metric's natural range).
    pub ranges: Vec<[f64; 2]>,
    /// Number of bins along each axis.
    pub bins: Vec<u32>,
    /// A short human-readable name for each axis, e.g. `fraction(Alive)`.
    pub labels: Vec<String>,
}

fn label_of(metric: &Metric) -> String {
    match metric {
        Metric::Fraction { types } => format!("fraction({})", types.join("+")),
        Metric::Activity => "activity".into(),
        Metric::Entropy => "entropy".into(),
        Metric::Lifetime => "lifetime".into(),
        Metric::TargetMask { .. } => "target_mask".into(),
        Metric::Series { .. } => "series".into(),
        Metric::DensityClassification { .. } => "density_classification".into(),
        Metric::BboxFraction { types } => format!("bbox({})", types.join("+")),
        Metric::Elongation { types } => format!("elongation({})", types.join("+")),
        Metric::CentroidSpeed { types } => format!("speed({})", types.join("+")),
        Metric::Growth { types } => format!("growth({})", types.join("+")),
        Metric::Period { window } => format!("period(≤{})", window / 2),
    }
}

impl Descriptor {
    /// Check the axes against a grid and fill in default ranges. One to three
    /// axes; every metric must be a descriptor (see [`Metric::is_descriptor`]).
    pub fn resolve(specs: &[DescriptorSpec], sim: &Sim, steps: u64) -> Result<Self, ModelError> {
        if specs.is_empty() || specs.len() > 3 {
            return Err(ModelError::InvalidParam(format!(
                "descriptors: give 1 to 3 axes, not {}",
                specs.len()
            )));
        }
        let mut ranges = Vec::new();
        let mut bins = Vec::new();
        let mut labels = Vec::new();
        for (i, s) in specs.iter().enumerate() {
            let label = label_of(&s.metric);
            if !s.metric.is_descriptor() {
                return Err(ModelError::InvalidParam(format!(
                    "descriptor {i} ({label}) compares against a target and cannot be an axis"
                )));
            }
            s.metric
                .validate(sim)
                .map_err(|e| ModelError::InvalidParam(format!("descriptor {i} ({label}): {e}")))?;
            if s.bins == 0 {
                return Err(ModelError::InvalidParam(format!(
                    "descriptor {i} ({label}): bins must be at least 1"
                )));
            }
            let range = s.range.unwrap_or_else(|| s.metric.range(sim, steps));
            if range[0].is_nan() || range[1].is_nan() || range[0] >= range[1] {
                return Err(ModelError::InvalidParam(format!(
                    "descriptor {i} ({label}): range [{}, {}] needs lo < hi",
                    range[0], range[1]
                )));
            }
            ranges.push(range);
            bins.push(s.bins);
            labels.push(label);
        }
        Ok(Descriptor {
            specs: specs.to_vec(),
            ranges,
            bins,
            labels,
        })
    }

    /// Number of axes.
    pub fn dims(&self) -> usize {
        self.specs.len()
    }

    /// Scale a raw descriptor onto `[0, 1]` per axis (clamped), so distances
    /// treat every axis alike.
    pub fn normalise(&self, d: &[f64]) -> Vec<f64> {
        d.iter()
            .zip(&self.ranges)
            .map(|(v, r)| ((v - r[0]) / (r[1] - r[0])).clamp(0.0, 1.0))
            .collect()
    }
}

/// A small picture of a grid: at most `max_side` cells on the long side, one
/// [`CellType`] per pixel, drawn by a gallery with the app's palette.
///
/// On the wire it is `{width, height, palette: [names], idx: [u8]}` so a
/// JSON report stays small: `idx` holds one palette position per pixel. With
/// more than 256 distinct types, the extras share the last palette slot; an
/// index past the palette reads back as the background type.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "ThumbnailWire", into = "ThumbnailWire")]
pub struct Thumbnail {
    /// Width in pixels (cells).
    pub width: u32,
    /// Height in pixels (cells).
    pub height: u32,
    /// Row-major pixels, `width * height` of them.
    pub cells: Vec<CellType>,
}

#[derive(Serialize, Deserialize)]
struct ThumbnailWire {
    width: u32,
    height: u32,
    palette: Vec<String>,
    idx: Vec<u8>,
}

impl From<Thumbnail> for ThumbnailWire {
    fn from(t: Thumbnail) -> Self {
        let mut palette: Vec<CellType> = Vec::new();
        let idx = t
            .cells
            .iter()
            .map(|c| {
                let pos = match palette.iter().position(|p| p == c) {
                    Some(p) => p,
                    None => {
                        palette.push(*c);
                        palette.len() - 1
                    }
                };
                pos.min(255) as u8
            })
            .collect();
        ThumbnailWire {
            width: t.width,
            height: t.height,
            palette: palette.iter().map(|c| c.as_str().to_string()).collect(),
            idx,
        }
    }
}

impl From<ThumbnailWire> for Thumbnail {
    fn from(w: ThumbnailWire) -> Self {
        let palette: Vec<CellType> = w.palette.iter().map(|n| CellType::new(n)).collect();
        let fallback = CellType::inactive();
        Thumbnail {
            width: w.width,
            height: w.height,
            cells: w
                .idx
                .iter()
                .map(|i| palette.get(*i as usize).copied().unwrap_or(fallback))
                .collect(),
        }
    }
}

/// Shrink a `width × height` block of cells to at most `max_side` on its long
/// side by nearest-neighbour sampling.
fn downsample(cells: &[CellType], width: usize, height: usize, max_side: u32) -> Thumbnail {
    let max_side = max_side.max(1) as usize;
    if width == 0 || height == 0 {
        return Thumbnail {
            width: 0,
            height: 0,
            cells: Vec::new(),
        };
    }
    let scale = (width.max(height) as f64 / max_side as f64).max(1.0);
    let tw = ((width as f64 / scale).round() as usize).clamp(1, width);
    let th = ((height as f64 / scale).round() as usize).clamp(1, height);
    let mut out = Vec::with_capacity(tw * th);
    for ty in 0..th {
        let sy = ((ty as f64 + 0.5) * height as f64 / th as f64) as usize;
        for tx in 0..tw {
            let sx = ((tx as f64 + 0.5) * width as f64 / tw as f64) as usize;
            out.push(cells[sy.min(height - 1) * width + sx.min(width - 1)]);
        }
    }
    Thumbnail {
        width: tw as u32,
        height: th as u32,
        cells: out,
    }
}

/// Build a 1D space-time thumbnail from rows recorded during a run (oldest
/// first): the picture is `width` across and one row per recorded step.
pub fn thumbnail_from_rows(width: usize, rows: &[Vec<CellType>], max_side: u32) -> Thumbnail {
    let flat: Vec<CellType> = rows.iter().flat_map(|r| r.iter().copied()).collect();
    downsample(&flat, width, rows.len(), max_side)
}

impl Sim {
    /// A thumbnail of the grid as it is now (a 1D grid gives a single row;
    /// use [`thumbnail_from_rows`] for a space-time strip).
    pub fn thumbnail(&self, max_side: u32) -> Thumbnail {
        let (w, h) = self.dims();
        downsample(self.cells(), w, h, max_side)
    }
}

/// The best genome found for one region of behaviour space.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Elite {
    /// The knob values, in the gene space's order.
    pub genome: super::genome::Genome,
    /// The same knob values as `key -> value`, for display and reports.
    pub named: BTreeMap<String, ParamValue>,
    /// The score (higher is better) that earned the cell.
    pub fitness: f64,
    /// Where it landed: one raw descriptor value per axis (before binning).
    pub descriptor: Vec<f64>,
    /// A picture of its final grid, when thumbnails are kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<Thumbnail>,
    /// The generation in which it was found.
    pub generation: u64,
}

/// What [`Archive::add`] did with a candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddOutcome {
    /// The cell was empty.
    Inserted,
    /// The cell had an elite and this one scored strictly higher.
    Improved,
    /// The cell's elite scored the same or higher (a tie keeps the incumbent).
    Rejected,
}

/// Summary numbers of an archive.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchiveStats {
    /// Cells holding an elite.
    pub elites: usize,
    /// `elites / cells`.
    pub coverage: f64,
    /// Sum over elites of `fitness − offset`.
    pub qd_score: f64,
    /// Best elite fitness (`-inf` when empty).
    pub obj_max: f64,
    /// Mean elite fitness (0 when empty).
    pub obj_mean: f64,
    /// Candidates whose descriptor fell outside an axis range (or was NaN)
    /// and were clamped to the edge cell. Many of these mean the range is
    /// wrong. A value exactly equal to an axis's upper bound is in range and
    /// does not count. (Result files from older versions counted it, so their
    /// `out_of_range` can be higher for the same run.)
    pub out_of_range: u64,
}

/// The grid of elites. See the module docs.
#[derive(Clone, Debug)]
pub struct Archive {
    /// Bins per axis; the cell count is their product.
    pub dims: Vec<u32>,
    /// `[low, high]` of each axis.
    pub ranges: Vec<[f64; 2]>,
    /// Axis names, as in [`Descriptor::labels`].
    pub labels: Vec<String>,
    cells: Vec<Option<Elite>>,
    qd_offset: f64,
    out_of_range: u64,
}

impl Archive {
    /// An empty archive over the descriptor's axes. `qd_offset` is subtracted
    /// from every fitness in the QD score; pass the lowest fitness possible
    /// (or 0 when there is no objective) so the score only ever grows.
    pub fn new(desc: &Descriptor, qd_offset: f64) -> Self {
        let n: usize = desc.bins.iter().map(|b| *b as usize).product();
        Archive {
            dims: desc.bins.clone(),
            ranges: desc.ranges.clone(),
            labels: desc.labels.clone(),
            cells: vec![None; n],
            qd_offset,
            out_of_range: 0,
        }
    }

    /// Total number of cells.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// True for an archive with no cells (never, after `new`).
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// The cell a descriptor falls in (row-major, first axis slowest) and
    /// whether it had to be clamped onto the grid's edge. Each axis is cut
    /// into equal bins over its range: with range `[0, 1]` and 4 bins, 0.26 is
    /// bin 1. Out-of-range and NaN values are clamped to the nearest edge bin
    /// (NaN goes to bin 0) and reported as `true`. A value exactly equal to the
    /// top of the range belongs to the last bin and is reported as `false`
    /// (it is in range; the maths would otherwise call it bin `n`). Older
    /// versions reported that case as clamped, so `out_of_range` counts in old
    /// result files can be higher than the same run gives now.
    pub fn cell_index(&self, descriptor: &[f64]) -> (usize, bool) {
        let mut index = 0usize;
        let mut clamped = false;
        for ((v, r), b) in descriptor.iter().zip(&self.ranges).zip(&self.dims) {
            let t = (v - r[0]) / (r[1] - r[0]);
            let mut bin = (t * f64::from(*b)).floor();
            if *v == r[1] {
                bin = f64::from(*b - 1);
            } else if !(0.0..f64::from(*b)).contains(&bin) {
                clamped = true;
                bin = bin.clamp(0.0, f64::from(*b - 1));
            }
            if bin.is_nan() {
                clamped = true;
                bin = 0.0;
            }
            index = index * (*b as usize) + bin as usize;
        }
        (index, clamped)
    }

    /// Bin coordinates of a cell index (inverse of [`Self::cell_index`]).
    pub fn coords(&self, mut index: usize) -> Vec<u32> {
        let mut out = vec![0u32; self.dims.len()];
        for (slot, b) in out.iter_mut().zip(&self.dims).rev() {
            *slot = (index % *b as usize) as u32;
            index /= *b as usize;
        }
        out
    }

    /// Offer a candidate; it stays only if its cell is empty or it scores
    /// strictly higher than the incumbent (a NaN fitness never beats one).
    pub fn add(&mut self, elite: Elite) -> AddOutcome {
        let (i, clamped) = self.cell_index(&elite.descriptor);
        if clamped {
            self.out_of_range += 1;
        }
        match &self.cells[i] {
            None => {
                self.cells[i] = Some(elite);
                AddOutcome::Inserted
            }
            Some(cur) if elite.fitness > cur.fitness => {
                self.cells[i] = Some(elite);
                AddOutcome::Improved
            }
            Some(_) => AddOutcome::Rejected,
        }
    }

    /// The elite in cell `i`, if any.
    pub fn get(&self, i: usize) -> Option<&Elite> {
        self.cells.get(i).and_then(|c| c.as_ref())
    }

    /// Every filled cell, with its index.
    pub fn elites(&self) -> impl Iterator<Item = (usize, &Elite)> {
        self.cells
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.as_ref().map(|e| (i, e)))
    }

    /// A uniformly random elite (by cell order), or `None` when empty.
    pub fn sample_elite(&self, rng: &mut Rng) -> Option<&Elite> {
        let filled: Vec<&Elite> = self.elites().map(|(_, e)| e).collect();
        if filled.is_empty() {
            return None;
        }
        Some(filled[rng.below(filled.len())])
    }

    /// The best elite by fitness, with its cell index (the later cell wins a tie).
    pub fn best(&self) -> Option<(usize, &Elite)> {
        self.elites().max_by(|a, b| {
            a.1.fitness
                .partial_cmp(&b.1.fitness)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Coverage, QD score and fitness summary of what is stored now.
    pub fn stats(&self) -> ArchiveStats {
        let fits: Vec<f64> = self.elites().map(|(_, e)| e.fitness).collect();
        let elites = fits.len();
        let obj_max = fits.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let obj_mean = if elites == 0 {
            0.0
        } else {
            fits.iter().sum::<f64>() / elites as f64
        };
        ArchiveStats {
            elites,
            coverage: if self.cells.is_empty() {
                0.0
            } else {
                elites as f64 / self.cells.len() as f64
            },
            qd_score: fits.iter().map(|f| f - self.qd_offset).sum(),
            obj_max,
            obj_mean,
            out_of_range: self.out_of_range,
        }
    }

    /// A cheap, cloneable copy for a viewer (thumbnails included when kept).
    pub fn snapshot(&self, generation: u64) -> ArchiveSnapshot {
        ArchiveSnapshot {
            dims: self.dims.clone(),
            ranges: self.ranges.clone(),
            labels: self.labels.clone(),
            cells: self
                .cells
                .iter()
                .map(|c| {
                    c.as_ref().map(|e| SnapshotCell {
                        fitness: e.fitness,
                        named: e.named.clone(),
                        thumbnail: e.thumbnail.clone(),
                    })
                })
                .collect(),
            stats: self.stats(),
            generation,
        }
    }

    /// A serializable report: stats, per-cell fitness, and every elite
    /// (with or without thumbnails).
    pub fn to_report(&self, thumbnails: bool) -> ArchiveReport {
        ArchiveReport {
            dims: self.dims.clone(),
            ranges: self.ranges.clone(),
            labels: self.labels.clone(),
            stats: self.stats(),
            cell_fitness: self
                .cells
                .iter()
                .map(|c| c.as_ref().map(|e| e.fitness))
                .collect(),
            elites: self
                .elites()
                .map(|(index, e)| ArchiveReportElite {
                    index,
                    coords: self.coords(index),
                    fitness: e.fitness,
                    descriptor: e.descriptor.clone(),
                    named: e.named.clone(),
                    thumbnail: if thumbnails {
                        e.thumbnail.clone()
                    } else {
                        None
                    },
                })
                .collect(),
        }
    }
}

/// One filled cell in an [`ArchiveSnapshot`].
#[derive(Clone, Debug, PartialEq)]
pub struct SnapshotCell {
    /// The elite's score.
    pub fitness: f64,
    /// Its knob values as `key -> value`.
    pub named: BTreeMap<String, ParamValue>,
    /// Its picture, if thumbnails are kept.
    pub thumbnail: Option<Thumbnail>,
}

/// A viewer's copy of an archive: what to draw, nothing to compute.
#[derive(Clone, Debug, PartialEq)]
pub struct ArchiveSnapshot {
    /// Bins per axis.
    pub dims: Vec<u32>,
    /// `[low, high]` of each axis.
    pub ranges: Vec<[f64; 2]>,
    /// Axis names.
    pub labels: Vec<String>,
    /// One entry per cell (row-major, as [`Archive::cell_index`]); `None` if empty.
    pub cells: Vec<Option<SnapshotCell>>,
    /// Summary numbers at the time of the snapshot.
    pub stats: ArchiveStats,
    /// The generation the snapshot was taken at.
    pub generation: u64,
}

/// One elite in an [`ArchiveReport`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchiveReportElite {
    /// The cell index (see [`Archive::cell_index`]).
    pub index: usize,
    /// The same cell as bin coordinates, one per axis.
    pub coords: Vec<u32>,
    /// The elite's score.
    pub fitness: f64,
    /// Its raw descriptor values.
    pub descriptor: Vec<f64>,
    /// Its knob values as `key -> value`.
    pub named: BTreeMap<String, ParamValue>,
    /// Its picture, if the report was asked to include thumbnails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<Thumbnail>,
}

/// The serializable form of an archive.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchiveReport {
    /// Bins per axis.
    pub dims: Vec<u32>,
    /// `[low, high]` of each axis.
    pub ranges: Vec<[f64; 2]>,
    /// Axis names.
    pub labels: Vec<String>,
    /// Summary numbers.
    pub stats: ArchiveStats,
    /// Fitness per cell (row-major); `None` for an empty cell.
    pub cell_fitness: Vec<Option<f64>>,
    /// Every filled cell.
    pub elites: Vec<ArchiveReportElite>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explore::genome::Genome;
    use crate::grid2d::Grid2D;
    use crate::rules::Rule2D;

    fn plain(w: usize, h: usize) -> Sim {
        let alive = CellType::from("Alive");
        let mut cells = vec![CellType::inactive(); w * h];
        cells[0] = alive;
        Sim::D2(Grid2D::new(w, h, 0, cells, Rule2D { subrules: vec![] }))
    }

    fn two_axes() -> Descriptor {
        Descriptor::resolve(
            &[
                DescriptorSpec {
                    bins: 4,
                    ..DescriptorSpec::new(Metric::Activity)
                },
                DescriptorSpec {
                    bins: 2,
                    range: Some([0.0, 2.0]),
                    ..DescriptorSpec::new(Metric::Entropy)
                },
            ],
            &plain(4, 4),
            10,
        )
        .unwrap()
    }

    fn elite(fit: f64, d: &[f64]) -> Elite {
        Elite {
            genome: Genome(vec![ParamValue::Float(fit)]),
            named: BTreeMap::new(),
            fitness: fit,
            descriptor: d.to_vec(),
            thumbnail: None,
            generation: 0,
        }
    }

    #[test]
    fn descriptor_resolves_ranges_labels_and_refuses_bad_axes() {
        let d = two_axes();
        assert_eq!(d.dims(), 2);
        assert_eq!(d.ranges, vec![[0.0, 1.0], [0.0, 2.0]]);
        assert_eq!(d.labels, vec!["activity", "entropy"]);
        assert_eq!(d.normalise(&[0.5, 3.0]), vec![0.5, 1.0]);
        let sim = plain(4, 4);
        assert!(Descriptor::resolve(&[], &sim, 10).is_err());
        let four = vec![DescriptorSpec::new(Metric::Activity); 4];
        assert!(Descriptor::resolve(&four, &sim, 10).is_err());
        let target = DescriptorSpec::new(Metric::TargetMask {
            types: vec![],
            mask: vec![],
            score: Default::default(),
        });
        let err = Descriptor::resolve(&[target], &sim, 10).unwrap_err();
        assert!(format!("{err}").contains("cannot be an axis"), "{err}");
        let ghost = DescriptorSpec::new(Metric::Fraction {
            types: vec!["Ghost".into()],
        });
        assert!(Descriptor::resolve(&[ghost], &sim, 10).is_err());
        let zero = DescriptorSpec {
            bins: 0,
            ..DescriptorSpec::new(Metric::Activity)
        };
        assert!(Descriptor::resolve(&[zero], &sim, 10).is_err());
        let flipped = DescriptorSpec {
            range: Some([1.0, 0.0]),
            ..DescriptorSpec::new(Metric::Activity)
        };
        assert!(Descriptor::resolve(&[flipped], &sim, 10).is_err());
        let lifetime =
            Descriptor::resolve(&[DescriptorSpec::new(Metric::Lifetime)], &sim, 77).unwrap();
        assert_eq!(lifetime.ranges, vec![[0.0, 77.0]]);
        assert_eq!(lifetime.labels, vec!["lifetime"]);
        let json = r#"{"metric": "period", "window": 32, "bins": 8}"#;
        let spec: DescriptorSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.metric, Metric::Period { window: 32 });
        assert_eq!(spec.bins, 8);
        let back: DescriptorSpec =
            serde_json::from_str(&serde_json::to_string(&spec).unwrap()).unwrap();
        assert_eq!(back, spec);
        assert_eq!(label_of(&Metric::Period { window: 32 }), "period(≤16)");
        assert_eq!(
            label_of(&Metric::Growth {
                types: vec!["A".into(), "B".into()]
            }),
            "growth(A+B)"
        );
    }

    #[test]
    fn cell_index_covers_edges_and_counts_out_of_range() {
        let mut a = Archive::new(&two_axes(), 0.0);
        assert_eq!(a.len(), 8);
        assert!(!a.is_empty());
        assert_eq!(a.cell_index(&[0.0, 0.0]), (0, false));
        assert_eq!(a.cell_index(&[0.26, 1.0]), (3, false));
        assert_eq!(
            a.cell_index(&[1.0, 2.0]),
            (3 * 2 + 1, false),
            "a value exactly at the top edge is in the last bin, not out of range"
        );
        assert_eq!(
            a.cell_index(&[1.0001, 2.0]),
            (3 * 2 + 1, true),
            "just above the top edge is out of range"
        );
        assert_eq!(a.cell_index(&[-1.0, 5.0]), (1, true));
        assert_eq!(a.cell_index(&[f64::NAN, 0.0]), (0, true));
        assert_eq!(a.coords(7), vec![3, 1]);
        assert_eq!(a.coords(2), vec![1, 0]);
        assert_eq!(a.add(elite(1.0, &[0.1, 0.1])), AddOutcome::Inserted);
        assert_eq!(a.add(elite(0.5, &[0.1, 0.1])), AddOutcome::Rejected);
        assert_eq!(a.add(elite(2.0, &[0.1, 0.1])), AddOutcome::Improved);
        assert_eq!(a.add(elite(-1.0, &[9.0, 9.0])), AddOutcome::Inserted);
        let s = a.stats();
        assert_eq!(s.elites, 2);
        assert_eq!(s.coverage, 0.25);
        assert_eq!(s.qd_score, 1.0, "2 + (-1) with offset 0");
        assert_eq!(s.obj_max, 2.0);
        assert_eq!(s.obj_mean, 0.5);
        assert_eq!(s.out_of_range, 1);
        assert_eq!(a.get(0).map(|e| e.fitness), Some(2.0));
        assert_eq!(a.get(1), None);
        assert_eq!(a.get(99), None);
        assert_eq!(a.best().map(|(i, e)| (i, e.fitness)), Some((0, 2.0)));
        // An offset makes negative fitness count positively.
        let mut b = Archive::new(&two_axes(), -3.0);
        b.add(elite(-1.0, &[0.1, 0.1]));
        b.add(elite(-2.0, &[0.6, 0.1]));
        assert_eq!(b.stats().qd_score, 3.0);
        let empty = Archive::new(&two_axes(), 0.0);
        let s = empty.stats();
        assert_eq!(
            (s.elites, s.coverage, s.qd_score, s.obj_mean),
            (0, 0.0, 0.0, 0.0)
        );
        assert_eq!(s.obj_max, f64::NEG_INFINITY);
        assert!(empty.best().is_none());
    }

    #[test]
    fn sampling_elites_is_deterministic_and_reports_round_trip() {
        let mut a = Archive::new(&two_axes(), 0.0);
        for (i, f) in [0.2, 0.9, 0.4].iter().enumerate() {
            a.add(elite(*f, &[i as f64 * 0.3, 0.5]));
        }
        assert!(
            Archive::new(&two_axes(), 0.0)
                .sample_elite(&mut Rng::new(1))
                .is_none()
        );
        let mut r1 = Rng::new(4);
        let mut r2 = Rng::new(4);
        for _ in 0..10 {
            assert_eq!(
                a.sample_elite(&mut r1).map(|e| e.fitness),
                a.sample_elite(&mut r2).map(|e| e.fitness)
            );
        }
        let snap = a.snapshot(3);
        assert_eq!(snap.generation, 3);
        assert_eq!(snap.cells.iter().filter(|c| c.is_some()).count(), 3);
        assert_eq!(snap.stats, a.stats());
        assert_eq!(snap.labels, a.labels);
        let report = a.to_report(false);
        assert_eq!(report.elites.len(), 3);
        assert_eq!(report.cell_fitness.iter().flatten().count(), 3);
        assert!(report.elites.iter().all(|e| e.thumbnail.is_none()));
        assert_eq!(report.elites[0].coords, a.coords(report.elites[0].index));
        let back: ArchiveReport =
            serde_json::from_str(&serde_json::to_string(&report).unwrap()).unwrap();
        assert_eq!(back, report);
    }

    #[test]
    fn thumbnails_downsample_and_round_trip_through_the_palette_wire_form() {
        let sim = plain(7, 3);
        let t = sim.thumbnail(64);
        assert_eq!(
            (t.width, t.height),
            (7, 3),
            "small grids are kept as they are"
        );
        assert_eq!(t.cells[0], CellType::from("Alive"));
        // A 10x10 live block at the origin survives a 3x shrink: the top-left
        // pixel samples the centre of its bin, cell (1, 1), which is inside it.
        let alive = CellType::from("Alive");
        let mut cells = vec![CellType::inactive(); 200 * 100];
        for y in 0..10 {
            for x in 0..10 {
                cells[y * 200 + x] = alive;
            }
        }
        let big = Sim::D2(Grid2D::new(200, 100, 0, cells, Rule2D { subrules: vec![] }));
        let t = big.thumbnail(64);
        assert_eq!((t.width, t.height), (64, 32));
        assert_eq!(t.cells.len(), 64 * 32);
        assert_eq!(
            t.cells[0], alive,
            "the top-left pixel reads inside the block"
        );
        assert_eq!(
            t.cells[63],
            CellType::inactive(),
            "the far end of the row is background"
        );
        assert_eq!(
            t.cells.iter().filter(|c| **c == alive).count(),
            9,
            "3x3 pixels of block"
        );
        let tall = plain(10, 300);
        let t = tall.thumbnail(64);
        assert_eq!((t.width, t.height), (2, 64));
        let empty = Sim::D2(Grid2D::new(0, 0, 0, vec![], Rule2D { subrules: vec![] }));
        assert_eq!(empty.thumbnail(64).cells.len(), 0);
        // 1D space-time strip: 5 rows of 100 cells become 64 x 3 (aspect kept).
        let x = CellType::from("X");
        let rows: Vec<Vec<CellType>> = (0..5)
            .map(|r| vec![if r % 2 == 0 { x } else { CellType::inactive() }; 100])
            .collect();
        let strip = thumbnail_from_rows(100, &rows, 64);
        assert_eq!((strip.width, strip.height), (64, 3));
        assert_eq!(thumbnail_from_rows(4, &[], 64).cells.len(), 0);
        // Wire form: palette + indices.
        let json = serde_json::to_string(&strip).unwrap();
        assert!(
            json.contains("\"palette\"") && json.contains("\"idx\""),
            "{json}"
        );
        let back: Thumbnail = serde_json::from_str(&json).unwrap();
        assert_eq!(back, strip);
        let bad: Thumbnail =
            serde_json::from_str(r#"{"width": 1, "height": 1, "palette": [], "idx": [7]}"#)
                .unwrap();
        assert_eq!(
            bad.cells,
            vec![CellType::inactive()],
            "an index past the palette falls back to the background"
        );
    }
}
