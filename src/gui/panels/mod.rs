//! One module per panel of the GUI. Each file owns the widgets for a single
//! region of the window and nothing else, so a change to (say) the statistics
//! chart cannot disturb the rule editor.

pub(in crate::gui) mod colors;
pub(in crate::gui) mod rule_edit_model;
pub(in crate::gui) mod rule_editor;
pub(in crate::gui) mod scenario;
pub(in crate::gui) mod statistics;
pub(in crate::gui) mod toolbar;
pub(in crate::gui) mod widgets;
