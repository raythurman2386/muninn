//! The deterministic layout and camera behind Muninn's graph view.
//!
//! This crate is deliberately free of Rune and windowing dependencies. It takes a
//! [`muninn_core::Vault`] and produces positions; the UI decides how to draw them.

mod camera;
mod layout;

pub use camera::{Camera, world_centre};
pub use layout::{GNode, Graph, NODE_H, NODE_W, WORLD};
