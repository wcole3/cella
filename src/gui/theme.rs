//! Design tokens and the theme switch.
//!
//! Every panel should look like it belongs to the same application. The way
//! to get that without a style guide nobody reads is to put the handful of
//! numbers that make up the look in one place and have every panel use them:
//! spacing steps, one corner radius, one accent colour. Think of this file as
//! the CSS variables of the app.
//!
//! [`apply`] pushes the tokens into egui's own style once per theme change;
//! [`section`] draws a titled block the way every panel draws one.

use egui::{Color32, CornerRadius};

/// Hairline gap, e.g. between a checkbox and its label.
pub(in crate::gui) const SPACE_XS: f32 = 2.0;
/// Gap between controls on one row.
pub(in crate::gui) const SPACE_SM: f32 = 4.0;
/// Gap between rows and after a section heading.
pub(in crate::gui) const SPACE_MD: f32 = 8.0;
/// Gap between sections.
pub(in crate::gui) const SPACE_LG: f32 = 12.0;
/// Corner radius of every widget.
pub(in crate::gui) const RADIUS: u8 = 4;
/// Selection / highlight colour on the dark theme.
pub(in crate::gui) const ACCENT_DARK: Color32 = Color32::from_rgb(0x3D, 0x9B, 0xE9);
/// Selection / highlight colour on the light theme.
pub(in crate::gui) const ACCENT_LIGHT: Color32 = Color32::from_rgb(0x1F, 0x6F, 0xC0);

/// Dark or light. The default matches egui's default (dark).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::gui) enum ThemeChoice {
    #[default]
    Dark,
    Light,
}

impl ThemeChoice {
    pub(in crate::gui) fn label(self) -> &'static str {
        match self {
            ThemeChoice::Dark => "Dark",
            ThemeChoice::Light => "Light",
        }
    }

    pub(in crate::gui) fn accent(self) -> Color32 {
        match self {
            ThemeChoice::Dark => ACCENT_DARK,
            ThemeChoice::Light => ACCENT_LIGHT,
        }
    }

    /// The background colour the viewport uses for `Inactive` cells unless
    /// the user picked another.
    pub(in crate::gui) fn default_inactive(self) -> Color32 {
        match self {
            ThemeChoice::Dark => Color32::from_rgb(30, 30, 35),
            ThemeChoice::Light => Color32::from_rgb(245, 245, 248),
        }
    }

    /// The default grid-line colour for the theme.
    pub(in crate::gui) fn default_grid_line(self) -> Color32 {
        match self {
            ThemeChoice::Dark => Color32::from_rgb(60, 60, 70),
            ThemeChoice::Light => Color32::from_rgb(200, 200, 210),
        }
    }

    fn egui_theme(self) -> egui::Theme {
        match self {
            ThemeChoice::Dark => egui::Theme::Dark,
            ThemeChoice::Light => egui::Theme::Light,
        }
    }
}

/// Push the tokens into egui's style for `theme` and select that theme.
/// Cheap enough to call on any theme change; do not call it every frame.
pub(in crate::gui) fn apply(ctx: &egui::Context, theme: ThemeChoice) {
    ctx.set_theme(theme.egui_theme());
    let accent = theme.accent();
    ctx.style_mut_of(theme.egui_theme(), |style| {
        style.spacing.item_spacing = egui::vec2(SPACE_MD, SPACE_SM);
        style.spacing.button_padding = egui::vec2(SPACE_MD, SPACE_SM);
        style.visuals.selection.bg_fill = accent;
        let radius = CornerRadius::same(RADIUS);
        for w in [
            &mut style.visuals.widgets.noninteractive,
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.open,
        ] {
            w.corner_radius = radius;
        }
    });
}

/// A named set of eight cell colours. Types take slots in order (see
/// `reset_colors_for_scenario`), so the first colour is the first declared
/// type's, and so on.
pub(in crate::gui) struct PalettePreset {
    pub name: &'static str,
    pub colors: [Color32; 8],
}

/// The palette presets offered in the Style tab. "Calm" is the palette the
/// app always had; Okabe-Ito and Tol are colour-blind-safe sets from the
/// literature; Viridis and Ember are sequential ramps for when types have an
/// order (fuel, burning, burnt).
pub(in crate::gui) const PALETTES: [PalettePreset; 5] = [
    PalettePreset {
        name: "Calm",
        colors: [
            Color32::from_rgb(0x56, 0xB4, 0xE9),
            Color32::from_rgb(0xE6, 0x9F, 0x00),
            Color32::from_rgb(0x00, 0xA9, 0xCF),
            Color32::from_rgb(0xF0, 0xE4, 0x42),
            Color32::from_rgb(0x66, 0xA6, 0x69),
            Color32::from_rgb(0xDF, 0x70, 0x93),
            Color32::from_rgb(0x80, 0x80, 0x80),
            Color32::from_rgb(0xAA, 0xCC, 0xEE),
        ],
    },
    PalettePreset {
        name: "Okabe-Ito",
        colors: [
            Color32::from_rgb(0xE6, 0x9F, 0x00),
            Color32::from_rgb(0x56, 0xB4, 0xE9),
            Color32::from_rgb(0x00, 0x9E, 0x73),
            Color32::from_rgb(0xF0, 0xE4, 0x42),
            Color32::from_rgb(0x00, 0x72, 0xB2),
            Color32::from_rgb(0xD5, 0x5E, 0x00),
            Color32::from_rgb(0xCC, 0x79, 0xA7),
            Color32::from_rgb(0x99, 0x99, 0x99),
        ],
    },
    PalettePreset {
        name: "Tol bright",
        colors: [
            Color32::from_rgb(0x44, 0x77, 0xAA),
            Color32::from_rgb(0xEE, 0x66, 0x77),
            Color32::from_rgb(0x22, 0x88, 0x33),
            Color32::from_rgb(0xCC, 0xBB, 0x44),
            Color32::from_rgb(0x66, 0xCC, 0xEE),
            Color32::from_rgb(0xAA, 0x33, 0x77),
            Color32::from_rgb(0xBB, 0xBB, 0xBB),
            Color32::from_rgb(0x77, 0x77, 0x77),
        ],
    },
    PalettePreset {
        name: "Viridis",
        colors: [
            Color32::from_rgb(0x44, 0x01, 0x54),
            Color32::from_rgb(0x46, 0x32, 0x7E),
            Color32::from_rgb(0x36, 0x5C, 0x8D),
            Color32::from_rgb(0x27, 0x7F, 0x8E),
            Color32::from_rgb(0x1F, 0xA1, 0x87),
            Color32::from_rgb(0x4A, 0xC1, 0x6D),
            Color32::from_rgb(0xA0, 0xDA, 0x39),
            Color32::from_rgb(0xFD, 0xE7, 0x25),
        ],
    },
    PalettePreset {
        name: "Ember",
        colors: [
            Color32::from_rgb(0x7F, 0x00, 0x00),
            Color32::from_rgb(0xB2, 0x22, 0x22),
            Color32::from_rgb(0xFF, 0x45, 0x00),
            Color32::from_rgb(0xFF, 0x8C, 0x00),
            Color32::from_rgb(0xFF, 0xD7, 0x00),
            Color32::from_rgb(0xFF, 0xF8, 0xDC),
            Color32::from_rgb(0x8B, 0x45, 0x13),
            Color32::from_rgb(0x4B, 0x2E, 0x05),
        ],
    },
];

/// A titled block: strong heading, the body, then a little air. Every panel
/// section goes through here so headings and gaps match everywhere.
pub(in crate::gui) fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(SPACE_SM);
    ui.strong(title);
    ui.add_space(SPACE_XS);
    body(ui);
    ui.add_space(SPACE_LG);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themes_have_distinct_defaults_and_apply_without_panicking() {
        assert_ne!(
            ThemeChoice::Dark.default_inactive(),
            ThemeChoice::Light.default_inactive()
        );
        assert_ne!(
            ThemeChoice::Dark.default_grid_line(),
            ThemeChoice::Light.default_grid_line()
        );
        assert_ne!(ThemeChoice::Dark.accent(), ThemeChoice::Light.accent());
        assert_eq!(ThemeChoice::default(), ThemeChoice::Dark);
        assert_eq!(ThemeChoice::Light.label(), "Light");
        let ctx = egui::Context::default();
        apply(&ctx, ThemeChoice::Light);
        assert_eq!(
            ctx.style_of(egui::Theme::Light).visuals.selection.bg_fill,
            ACCENT_LIGHT
        );
        apply(&ctx, ThemeChoice::Dark);
        assert_eq!(
            ctx.style_of(egui::Theme::Dark).visuals.selection.bg_fill,
            ACCENT_DARK
        );
        assert_eq!(
            ctx.style_of(egui::Theme::Dark)
                .visuals
                .widgets
                .inactive
                .corner_radius,
            CornerRadius::same(RADIUS)
        );
        egui::__run_test_ui(|ui| {
            section(ui, "Title", |ui| {
                ui.label("body");
            })
        });
    }

    #[test]
    fn palettes_are_named_and_each_has_eight_distinct_colours() {
        let mut names = Vec::new();
        for p in &PALETTES {
            assert!(!p.name.is_empty());
            names.push(p.name);
            let mut cols = p.colors.to_vec();
            cols.sort_by_key(|c| c.to_array());
            cols.dedup();
            assert_eq!(cols.len(), 8, "{} repeats a colour", p.name);
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), PALETTES.len());
        assert_eq!(
            PALETTES[0].colors.to_vec(),
            crate::gui::render::default_palette(),
            "Calm is the historical default"
        );
    }
}
