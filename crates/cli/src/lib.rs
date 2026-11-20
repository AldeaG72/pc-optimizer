//! `pco` — command-line interface for PC Optimizer.
//!
//! The binary (`main.rs`) only wires real Windows backends into [`app::run`]; everything
//! else lives here so tests can run it with in-memory backends.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod app;
pub mod args;
pub mod format;
pub mod i18n;

/// The recipes compiled into the program (see `build.rs`).
pub mod embedded {
    include!(concat!(env!("OUT_DIR"), "/recipes.rs"));
}
