//! Thin module shim for GUI composed of submodules in `src/gui/`.
//! This preserves the `mod gui;` usage from main.rs while allowing
//! the code to be split across files.

mod actions;
mod app;
mod explore;
mod export;
mod gallery;
mod interact;
mod layers;
mod painter;
mod panels;
mod patterns;
mod render;
mod scenarios;
mod shortcuts;
mod sim;
mod state;
mod theme;
mod types;

pub use app::run_gui;
