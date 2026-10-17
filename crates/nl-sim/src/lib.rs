//! The Nine Lives simulation.
//!
//! This crate is the only place where physics, thruster forces, the air tank,
//! events, observations and rewards are computed. Python (through `nl-py`) and
//! the browser (through `nl-wasm`) both call into it.

pub mod car;
pub mod conventions;
pub mod golden_trace;
pub mod hop_boost;
pub mod input;
pub mod modes;
pub mod tank;
pub mod thrusters;
pub mod vehicle_config;
pub mod world;
