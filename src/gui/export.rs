use std::collections::{BTreeMap, HashMap};

use cella_lib::*;
use egui::Color32;

/// Build a stable palette mapping for GIF export.
///
/// Index 0 is reserved for the Inactive color. Subsequent indices are assigned
/// to distinct type names present in the provided `types` slice. Colors are
/// chosen from `colors` overrides when present, otherwise from hashing into the
/// provided `fallback_palette`.
fn build_palette_map(
    types: &[CellType],
    colors: &HashMap<String, Color32>,
    fallback_palette: &Vec<Color32>,
    inactive: Color32,
) -> (Vec<u8>, BTreeMap<String, u8>) {
    let mut color_table: Vec<u8> = Vec::with_capacity(256 * 3);

    // Index 0 = inactive
    color_table.push(inactive.r());
    color_table.push(inactive.g());
    color_table.push(inactive.b());

    // Assign indices 1.. to distinct types (excluding Inactive)
    let mut map: BTreeMap<String, u8> = BTreeMap::new();
    let mut next_index: u8 = 1;
    for ty in types {
        if ty.0 == INACTIVE { continue; }
        if map.contains_key(&ty.0) { continue; }
        let col = if let Some(c) = colors.get(&ty.0) { *c } else {
            // fallback by hashing name into palette slot
            let mut h: u64 = 0xcbf29ce484222325; let prime: u64 = 0x00000100000001B3;
            for &b in ty.0.as_bytes() { h ^= b as u64; h = h.wrapping_mul(prime); }
            let idx = (h as usize) % fallback_palette.len().max(1);
            fallback_palette.get(idx).copied().unwrap_or(Color32::LIGHT_BLUE)
        };
        color_table.push(col.r());
        color_table.push(col.g());
        color_table.push(col.b());
        map.insert(ty.0.clone(), next_index);
        next_index = next_index.saturating_add(1);
        if next_index == 0 { break; } // avoid overflow; unlikely with few types
    }

    // Fill the rest of the 256-color table with repeats of fallback palette
    while color_table.len() < 256 * 3 {
        let idx = ((color_table.len()/3) - 1) % fallback_palette.len().max(1);
        let c = fallback_palette.get(idx).copied().unwrap_or(Color32::LIGHT_BLUE);
        color_table.push(c.r());
        color_table.push(c.g());
        color_table.push(c.b());
    }

    (color_table, map)
}

/// Export a 2D grid to an animated GIF. Colors match the GUI mapping,
/// including the configurable Inactive color.
pub fn export_gif_2d(
    grid: &mut Grid2D,
    path: std::path::PathBuf,
    steps: usize,
    fps: u32,
    scale: u16,
    colors: &HashMap<String, Color32>,
    palette: &Vec<Color32>,
    inactive: Color32,
) -> Result<(), Box<dyn std::error::Error>> {
    use gif::{Encoder, Frame};
    let w = (grid.width as u16).saturating_mul(scale);
    let h = (grid.height as u16).saturating_mul(scale);

    // Gather current distinct types
    let mut set: BTreeMap<String, CellType> = BTreeMap::new();
    for c in &grid.cells { set.entry(c.current.0.clone()).or_insert(c.current.clone()); }
    let types: Vec<CellType> = set.values().cloned().collect();

    let (color_table, index_map) = build_palette_map(&types, colors, palette, inactive);

    let mut file = std::fs::File::create(path)?;
    let mut encoder = Encoder::new(&mut file, w, h, &color_table)?;
    let delay_cs = (100.0 / (fps.max(1) as f32)).round() as u16;

    for _ in 0..steps {
        let mut buf = vec![0u8; (w as usize) * (h as usize)];
        for y in 0..grid.height {
            for x in 0..grid.width {
                let idx = y * grid.width + x;
                let ty = &grid.cells[idx].current;
                let pal_index = if ty.0 == INACTIVE { 0u8 } else { *index_map.get(&ty.0).unwrap_or(&1u8) };
                for dy in 0..scale as usize {
                    for dx in 0..scale as usize {
                        let px = (x) * (scale as usize) + dx;
                        let py = (y) * (scale as usize) + dy;
                        buf[py * (w as usize) + px] = pal_index;
                    }
                }
            }
        }
        let mut frame = Frame::default();
        frame.width = w; frame.height = h; frame.delay = delay_cs; frame.buffer = std::borrow::Cow::Owned(buf);
        encoder.write_frame(&frame)?;
        grid.step();
    }

    Ok(())
}

/// Export a 1D grid to an animated GIF. Colors match the GUI mapping,
/// including the configurable Inactive color.
pub fn export_gif_1d(
    grid: &mut Grid1D,
    path: std::path::PathBuf,
    steps: usize,
    fps: u32,
    scale: u16,
    colors: &HashMap<String, Color32>,
    palette: &Vec<Color32>,
    inactive: Color32,
) -> Result<(), Box<dyn std::error::Error>> {
    use gif::{Encoder, Frame};
    let w = (grid.width as u16).saturating_mul(scale);
    let h = 1u16.saturating_mul(scale);

    // Gather current distinct types
    let mut set: BTreeMap<String, CellType> = BTreeMap::new();
    for c in &grid.cells { set.entry(c.current.0.clone()).or_insert(c.current.clone()); }
    let types: Vec<CellType> = set.values().cloned().collect();

    let (color_table, index_map) = build_palette_map(&types, colors, palette, inactive);

    let mut file = std::fs::File::create(path)?;
    let mut encoder = Encoder::new(&mut file, w, h, &color_table)?;

    let delay_cs = (100.0 / (fps.max(1) as f32)).round() as u16;

    for _ in 0..steps {
        let mut buf = vec![0u8; (w as usize) * (h as usize)];
        for x in 0..grid.width {
            let ty = &grid.cells[x].current;
            let pal_index = if ty.0 == INACTIVE { 0u8 } else { *index_map.get(&ty.0).unwrap_or(&1u8) };
            for dy in 0..scale as usize {
                for dx in 0..scale as usize {
                    let px = (x) * (scale as usize) + dx;
                    let py = 0usize * (scale as usize) + dy;
                    buf[py * (w as usize) + px] = pal_index;
                }
            }
        }
        let mut frame = Frame::default();
        frame.width = w; frame.height = h; frame.delay = delay_cs; frame.buffer = std::borrow::Cow::Owned(buf);
        encoder.write_frame(&frame)?;
        grid.step();
    }

    Ok(())
}
