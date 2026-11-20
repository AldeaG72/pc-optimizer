//! `pco` — command-line interface for PC Optimizer.
//!
//! The binary (`main.rs`) only wires real Windows backends into [`app::run`]; everything
//! else lives here so tests can run it with in-memory backends.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod app;
pub mod args;
pub mod bench;
#[cfg(windows)]
pub mod bench_windows;
pub mod format;
pub mod health;
pub mod i18n;
pub mod testing;

/// The recipes compiled into the program (see `build.rs`).
pub mod embedded {
    include!(concat!(env!("OUT_DIR"), "/recipes.rs"));
}
