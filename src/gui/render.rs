//! Rendering utilities for the GUI.
//!
//! Currently provides the default color palette used to distinguish active
//! cell types in the grid viewport and GIF export.

use egui::Color32;

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
