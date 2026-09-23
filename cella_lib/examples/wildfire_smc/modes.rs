//! One sub-module per CLI mode (`open`/`assim`/`evolve`/`map`, plus
//! `replay`, which re-evaluates a `map`-mode archive and so lives beside
//! it in `map.rs`). `nulls` mode lives in `crate::nulls` instead, next to
//! the null-forecaster code it is entirely built from.

pub(crate) mod assim;
pub(crate) mod evolve;
pub(crate) mod map;
pub(crate) mod open;
