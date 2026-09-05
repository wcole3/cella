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
        Color32::from_rgb(0x56, 0xB4, 0xE9), // sky
        Color32::from_rgb(0xE6, 0x9F, 0x00), // orange
        Color32::from_rgb(0x00, 0xA9, 0xCF), // teal
        Color32::from_rgb(0xF0, 0xE4, 0x42), // yellow
        Color32::from_rgb(0x66, 0xA6, 0x69), // green
        Color32::from_rgb(0xDF, 0x70, 0x93), // rose
        Color32::from_rgb(0x80, 0x80, 0x80), // gray
        Color32::from_rgb(0xAA, 0xCC, 0xEE), // light blue
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

/// Parse `#rrggbb` (or `rrggbb`) into a colour; anything else is `None`.
pub fn parse_hex_color(s: &str) -> Option<Color32> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?))
}

/// One palette slot per name, with no two names sharing a slot while slots
/// last: the i-th name gets slot i. Past the palette's length there is nothing
/// left to hand out, so those names fall back to the hashed slot.
///
/// Callers pass names in a stable order (the GUI uses `declared_types`, which
/// sorts alphabetically), so a scenario gets the same colours every time.
pub fn distinct_palette_slots(names: &[&str], palette_len: usize) -> Vec<usize> {
    names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            if i < palette_len {
                i
            } else {
                palette_index_for(name, palette_len)
            }
        })
        .collect()
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
    if ty == CellType::inactive() {
        return inactive;
    }
    if let Some(&c) = colors.get(&ty.0) {
        return c;
    }
    let idx = palette_index_for(ty.as_str(), palette.len());
    palette.get(idx).copied().unwrap_or(Color32::LIGHT_BLUE)
}

/// Linear blend between two colours (`t` in `0..=1`).
fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()))
}

/// Colour ramp for a probability or other `0..=1` weight: cool blue at 0,
/// yellow at 0.5, red at 1.
pub fn heat_color(t: f32) -> Color32 {
    let blue = Color32::from_rgb(0x2B, 0x83, 0xBA);
    let yellow = Color32::from_rgb(0xFF, 0xD7, 0x00);
    let red = Color32::from_rgb(0xD7, 0x19, 0x1C);
    if t < 0.5 {
        lerp(blue, yellow, t * 2.0)
    } else {
        lerp(yellow, red, (t - 0.5) * 2.0)
    }
}

/// Colour ramp for cell age, `t = 1` meaning "changed just now" (bright
/// yellow), fading through orange to deep violet at `t = 0`.
pub fn age_color(t: f32) -> Color32 {
    let cold = Color32::from_rgb(0x3B, 0x0F, 0x70);
    let warm = Color32::from_rgb(0xFF, 0x45, 0x00);
    let hot = Color32::from_rgb(0xFD, 0xE7, 0x25);
    if t < 0.5 {
        lerp(cold, warm, t * 2.0)
    } else {
        lerp(warm, hot, (t - 0.5) * 2.0)
    }
}

/// The same colour at `alpha` opacity (`0..=1`).
pub fn with_opacity(c: Color32, alpha: f32) -> Color32 {
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_color_reads_rrggbb_with_or_without_hash() {
        assert_eq!(
            parse_hex_color("#2e8b57"),
            Some(Color32::from_rgb(0x2e, 0x8b, 0x57))
        );
        assert_eq!(
            parse_hex_color("FF4500"),
            Some(Color32::from_rgb(0xff, 0x45, 0x00))
        );
    }

    #[test]
    fn parse_hex_color_rejects_anything_else() {
        assert_eq!(parse_hex_color(""), None);
        assert_eq!(parse_hex_color("#12345"), None);
        assert_eq!(parse_hex_color("#gg0000"), None);
        assert_eq!(parse_hex_color("red"), None);
    }

    #[test]
    fn distinct_palette_slots_gives_each_name_its_own_slot() {
        // These four collide pairwise under the hash (0,0,3,3); walking free
        // slots in order must separate them.
        let names = ["BurnedOut", "Burning", "Forest", "Shrub"];
        let slots = distinct_palette_slots(&names, 8);
        assert_eq!(slots, vec![0, 1, 2, 3]);
    }

    #[test]
    fn distinct_palette_slots_wraps_to_the_hash_past_the_palette() {
        let names: Vec<String> = (0..9).map(|i| format!("T{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let slots = distinct_palette_slots(&refs, 8);
        assert_eq!(slots[..8], [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(
            slots[8],
            palette_index_for("T8", 8),
            "9th falls back to the hash"
        );
    }

    #[test]
    fn ramps_run_the_documented_way_and_opacity_sets_alpha() {
        assert!(heat_color(0.0).b() > heat_color(1.0).b(), "blue fades out");
        assert!(heat_color(1.0).r() > heat_color(0.0).r(), "red comes in");
        assert_eq!(heat_color(0.5), Color32::from_rgb(0xFF, 0xD7, 0x00));
        assert!(age_color(1.0).g() > age_color(0.0).g(), "hot is bright");
        assert_eq!(age_color(0.0), Color32::from_rgb(0x3B, 0x0F, 0x70));
        assert_eq!(age_color(2.0), age_color(1.0), "clamped");
        let c = with_opacity(Color32::WHITE, 0.5);
        assert_eq!(c.a(), 128);
        assert_eq!(with_opacity(Color32::WHITE, 7.0).a(), 255);
    }
}
