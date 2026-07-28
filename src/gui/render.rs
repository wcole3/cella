//! Rendering utilities shared by the GUI viewport and the GIF exporter.
//!
//! Provides the default color palette used to distinguish active cell types and
//! the single source of truth for resolving a cell type to a color, so what is
//! painted on screen and what is written into an exported GIF cannot drift apart.

use std::collections::HashMap;

use cella_lib::CellType;
use egui::Color32;
use lasso2::Spur;

/// Default UI color palette used for active cell types when no explicit
/// color is assigned by the user.
pub fn default_palette() -> Vec<Color32> {
    // Calm, high-contrast but not harsh palette
    vec![
        Color32::from_rgb(0x56,0xB4,0xE9), // sky
        Color32::from_rgb(0xE6,0x9F,0x00), // orange
        Color32::from_rgb(0x00,0xA9,0xCF), // teal
        Color32::from_rgb(0xF0,0xE4,0x42), // yellow
        Color32::from_rgb(0x66,0xA6,0x69), // green
        Color32::from_rgb(0xDF,0x70,0x93), // rose
        Color32::from_rgb(0x80,0x80,0x80), // gray
        Color32::from_rgb(0xAA,0xCC,0xEE), // light blue
    ]
}

/// Deterministic FNV-1a hash of a type name, used to pick a fallback palette
/// slot when the user has not assigned an explicit color.
pub fn palette_index_for(name: &str, palette_len: usize) -> usize {
    let mut h: u64 = 0xcbf29ce484222325; // FNV offset basis
    const PRIME: u64 = 0x00000100000001B3; // FNV prime
    for &b in name.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(PRIME);
    }
    (h as usize) % palette_len.max(1)
}

/// Resolve the display color for `ty`.
///
/// Precedence: the configurable Inactive color, then an explicit user override
/// from `colors`, then a deterministic slot in `palette`. Both the viewport and
/// the GIF exporter go through here, so their colors always agree.
pub fn color_for(
    ty: CellType,
    colors: &HashMap<Spur, Color32>,
    palette: &[Color32],
    inactive: Color32,
) -> Color32 {
    if ty == CellType::inactive() { return inactive; }
    if let Some(&c) = colors.get(&ty.0) { return c; }
    let idx = palette_index_for(ty.as_str(), palette.len());
    palette.get(idx).copied().unwrap_or(Color32::LIGHT_BLUE)
}
