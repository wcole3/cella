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
}
