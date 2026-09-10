//! Visual and interaction testing.
//!
//! Everything here runs the real application against a headless egui context,
//! so the tests need no window, no display server and no control of the machine
//! they run on: `cargo test` is enough.
//!
//! Unit tests live next to the code they cover. This module holds the harness
//! and the tests that need it.

mod harness;
mod interaction;
mod visual;
