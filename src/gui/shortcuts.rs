//! Every keyboard shortcut, in one table.
//!
//! The table is read three times: by the hotkey handler (to fire the action),
//! by the toolbar (to print "Play/Pause (Space)" in a tooltip), and by the `?`
//! overlay (to list them all). One table, so the three can never disagree.

use egui::{Key, KeyboardShortcut, Modifiers};

use super::actions::Action;
use super::layers::Layer;

/// One key binding.
pub(in crate::gui) struct Shortcut {
    pub keys: KeyboardShortcut,
    /// What the `?` overlay shows.
    pub label: &'static str,
    pub action: Action,
    /// Editing keys are ignored while the simulation plays; playback and
    /// view keys work either way.
    pub while_playing: bool,
}

fn key(k: Key, label: &'static str, action: Action) -> Shortcut {
    Shortcut {
        keys: KeyboardShortcut::new(Modifiers::NONE, k),
        label,
        action,
        while_playing: true,
    }
}

fn cmd(k: Key, label: &'static str, action: Action) -> Shortcut {
    Shortcut {
        keys: KeyboardShortcut::new(Modifiers::COMMAND, k),
        label,
        action,
        while_playing: true,
    }
}

/// The table. `Action` holds a `String` variant, so this is a function rather
/// than a `const`.
pub(in crate::gui) fn shortcuts() -> Vec<Shortcut> {
    vec![
        key(Key::Space, "Play / pause", Action::TogglePlay),
        key(Key::S, "Step once", Action::Step),
        key(Key::ArrowRight, "Step once", Action::Step),
        cmd(Key::R, "Reset to the initial state", Action::Reset),
        key(Key::Plus, "Zoom in", Action::ZoomIn),
        key(Key::Equals, "Zoom in", Action::ZoomIn),
        key(Key::Minus, "Zoom out", Action::ZoomOut),
        key(Key::F, "Zoom to fit", Action::ZoomToFit),
        key(Key::G, "Toggle grid lines", Action::ToggleGridLines),
        key(
            Key::A,
            "Toggle the age heat layer",
            Action::ToggleLayer(Layer::Age),
        ),
        key(
            Key::P,
            "Toggle the probability layer",
            Action::ToggleLayer(Layer::Probability),
        ),
        key(Key::L, "Show / hide the left panel", Action::ToggleLeft),
        key(Key::W, "Show / hide the right panel", Action::ToggleRight),
        key(Key::Questionmark, "Show this list", Action::ToggleShortcuts),
        cmd(Key::E, "Export a GIF", Action::ExportGif),
        cmd(Key::S, "Save the current state", Action::SaveFinalState),
        Shortcut {
            keys: KeyboardShortcut::new(Modifiers::COMMAND, Key::Z),
            label: "Undo the last edit",
            action: Action::Undo,
            while_playing: false,
        },
        Shortcut {
            keys: KeyboardShortcut::new(Modifiers::COMMAND, Key::U),
            label: "Undo the last rule change",
            action: Action::UndoRule,
            while_playing: false,
        },
        Shortcut {
            keys: KeyboardShortcut::new(Modifiers::NONE, Key::R),
            label: "Random fill with the Edit tab's settings",
            action: Action::RandomFillDraft,
            while_playing: false,
        },
        Shortcut {
            keys: KeyboardShortcut::new(Modifiers::SHIFT, Key::R),
            label: "Surprise me: random knobs and a random fill",
            action: Action::SurpriseMeDraft,
            while_playing: false,
        },
        Shortcut {
            keys: KeyboardShortcut::new(Modifiers::NONE, Key::M),
            label: "Mutate the rule a little",
            action: Action::MutateRuleDraft,
            while_playing: false,
        },
        Shortcut {
            keys: KeyboardShortcut::new(Modifiers::NONE, Key::CloseBracket),
            label: "Bigger brush",
            action: Action::BrushGrow,
            while_playing: false,
        },
        Shortcut {
            keys: KeyboardShortcut::new(Modifiers::NONE, Key::OpenBracket),
            label: "Smaller brush",
            action: Action::BrushShrink,
            while_playing: false,
        },
    ]
}

/// The first key bound to `action`, formatted for the platform ("Ctrl+R"),
/// for tooltips. `None` if the action has no shortcut.
pub(in crate::gui) fn shortcut_text(ctx: &egui::Context, action: &Action) -> Option<String> {
    shortcuts()
        .into_iter()
        .find(|s| s.action == *action)
        .map(|s| ctx.format_shortcut(&s.keys))
}

/// Tooltip text for a control: `label`, plus its shortcut in brackets when
/// it has one.
pub(in crate::gui) fn tooltip(ctx: &egui::Context, label: &str, action: &Action) -> String {
    match shortcut_text(ctx, action) {
        Some(k) => format!("{label} ({k})"),
        None => label.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::sim::tests::test_app;

    #[test]
    fn no_two_bindings_share_a_key_combination() {
        let table = shortcuts();
        for (i, a) in table.iter().enumerate() {
            for b in &table[i + 1..] {
                assert!(a.keys != b.keys, "{:?} is bound twice", a.keys);
            }
        }
        assert!(
            table.iter().any(|s| !s.while_playing),
            "editing keys pause-only"
        );
    }

    #[test]
    fn every_bound_action_is_handled_by_the_reducer() {
        let mut app = test_app();
        app.load_demo_life();
        for s in shortcuts() {
            // Dialog-opening actions would block a headless test.
            if matches!(s.action, Action::ExportGif | Action::SaveFinalState) {
                continue;
            }
            app.apply_action(s.action.clone());
        }
        app.drain_actions();
    }

    #[test]
    fn tooltips_carry_the_shortcut_when_there_is_one() {
        let ctx = egui::Context::default();
        let t = tooltip(&ctx, "Play", &Action::TogglePlay);
        assert!(t.starts_with("Play (") && t.contains("Space"), "{t}");
        assert_eq!(tooltip(&ctx, "Run", &Action::RunTo { steps: 5 }), "Run");
        assert!(shortcut_text(&ctx, &Action::Reset).unwrap().contains('R'));
    }
}
