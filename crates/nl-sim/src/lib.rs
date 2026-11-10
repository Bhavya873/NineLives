//! The Nine Lives simulation.
//!
//! This crate is the only place where physics, thruster forces, the air tank,
//! events, observations and rewards are computed. Python (through `nl-py`) and
//! the browser (through `nl-wasm`) both call into it.

pub mod actuator_model;
pub mod car;
pub mod car_state;
pub mod chaos;
pub mod checkpoint;
pub mod contacts;
pub mod conventions;
pub mod course;
pub mod course_build;
pub mod course_gen;
pub mod course_mesh;
pub mod course_triggers;
pub mod diagnostics;
pub mod driver_bot;
pub mod driving_stages;
pub mod episode;
pub mod events;
pub mod flight_stage;
mod floor_hook;
pub mod forecast;
pub mod g_load;
pub mod golden_trace;
pub mod hash;
pub mod hop_boost;
pub mod hop_profile;
pub mod input;
pub mod landing;
pub mod modes;
pub mod obs;
pub mod provenance;
pub mod randomization;
pub mod reward;
pub mod scenarios;
pub mod spin_axis;
mod stage_start;
pub mod tank;
pub mod thrusters;
pub mod training;
pub mod traversal;
pub mod vehicle_config;
pub mod wind;
pub mod world;
