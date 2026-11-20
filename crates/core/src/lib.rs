//! Core logic for PC Optimizer: recipes, engine, undo journal and Windows backends.
//!
//! This crate has no UI. Everything that touches Windows lives behind traits so that
//! most tests run against in-memory backends on any OS.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod recipe;

/// Crate version, taken from the workspace `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        assert!(!VERSION.is_empty());
    }
}
