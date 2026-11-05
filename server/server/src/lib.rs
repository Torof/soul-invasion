//! Soul Invasion game server library.
//!
//! The binary in `main.rs` is a thin wrapper around this crate so that the
//! simulation loop can be tested in-process. Milestone 1 ("the skeleton")
//! adds, step by step, the time loop (this), the WebSocket listener and the
//! storage layer (D28, D29).

pub mod cli;
pub mod run;

pub use cli::Options;
pub use run::{RunSummary, run_headless, run_paced};
