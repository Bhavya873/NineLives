//! The trained policy, run in Rust.
//!
//! M5 adds the `models/policy.json` loader, observation normalisation and the
//! MLP forward pass (SPEC §9.12). For now it holds only the format name, so the
//! crate builds and the exporter has a constant to match.

/// The `format` field every exported policy file must carry (SPEC §9.12).
pub const POLICY_FORMAT: &str = "ninelives-policy";

/// The policy file version this crate can read.
pub const POLICY_FORMAT_VERSION: u32 = 1;
