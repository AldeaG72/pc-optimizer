//! `pco-helper`: the only part of PC Optimizer that runs with administrator rights.
//!
//! The app runs as a normal user. When the user clicks "Apply all" (or "Undo"), the app
//! starts `pco-helper.exe` with UAC, and the two talk over a named pipe that only the
//! current user can open. The helper takes one request made of recipe IDs, runs it with
//! the same guards and journal as `pco`, reports each result, and exits.
//!
//! - [`protocol`]: the request and event format, and the checks on a request;
//! - [`serve`]: runs a request against the engine (any backends, so tests run anywhere);
//! - [`pipe`] (Windows): the channel, the checks on the other end, starting the helper.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod bench;
#[cfg(windows)]
pub mod pipe;
pub mod protocol;
pub mod serve;
