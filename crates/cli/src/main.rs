//! `pco` — command-line interface for PC Optimizer.
//!
//! Commands (`list`, `plan`, `apply`, `undo`, `status`) arrive in later Stage 0 steps.

fn main() {
    println!("pco {}", pco_core::VERSION);
}
