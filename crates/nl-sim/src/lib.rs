//! The Nine Lives simulation.
//!
//! This crate is the only place where physics, thruster forces, the air tank,
//! events, observations and rewards are computed. Python (through `nl-py`) and
//! the browser (through `nl-wasm`) both call into it.

pub mod car;
pub mod car_state;
pub mod chaos;
pub mod checkpoint;
pub mod contacts;
pub mod conventions;
pub mod course;
pub mod course_build;
pub mod episode;
pub mod events;
pub mod golden_trace;
pub mod hash;
pub mod hop_boost;
pub mod input;
pub mod modes;
pub mod obs;
pub mod randomization;
pub mod reward;
pub mod scenarios;
pub mod tank;
pub mod thrusters;
pub mod training;
pub mod vehicle_config;
pub mod world;
