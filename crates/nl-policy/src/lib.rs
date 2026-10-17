//! The trained policy, run in Rust (SPEC §8 "Ai", §9.12).
//!
//! `Policy::from_json` reads `models/policy.json` (written by `ninelives.export`) and
//! refuses a file made for another observation layout; `Policy::act` normalizes an
//! observation and runs the small MLP. The eval runner and the browser both use it, so
//! the AI behaves the same everywhere.

mod policy;

pub use policy::{Policy, PolicyError};

/// The `format` field every exported policy file must carry (SPEC §9.12).
pub const POLICY_FORMAT: &str = "ninelives-policy";

/// The policy file version this crate can read.
pub const POLICY_FORMAT_VERSION: u32 = 1;
