//! The golden trace: a fixed, scripted drive used to prove determinism (SPEC §7.10).
//!
//! The same 2000 steps of scripted input are run on every platform. After every step
//! the car's pose and velocities are folded into a 64-bit FNV-1a hash of their exact
//! bits, and at the end the whole world snapshot is folded in too, so any difference
//! anywhere changes the hash. The expected result is stored in
//! `crates/nl-sim/tests/golden/flat_drive.json`, recorded with the frozen car in
//! `tests/golden/vehicle.toml`. `cargo test` checks it natively; M2 adds the same
//! check for the wasm build.

use serde::{Deserialize, Serialize};

use crate::conventions::{NOZZLE_COUNT, PHYSICS_HZ, PHYSICS_STEPS_PER_CONTROL_STEP};
use crate::input::InputFrame;
use crate::vehicle_config::{VehicleConfig, VehicleConfigError};
use crate::world::{DEFAULT_GROUND_FRICTION, SnapshotError, World};

/// Number of physics steps in the golden trace (SPEC §7.10).
pub const GOLDEN_TRACE_STEPS: u32 = 2000;

/// The frozen car the golden hash was recorded with. Tuning `configs/vehicle.toml`
/// must not change the golden hash, so the trace uses this copy instead.
pub const GOLDEN_VEHICLE_TOML: &str = include_str!("../tests/golden/vehicle.toml");

/// FNV-1a 64-bit starting value and multiplier (the standard constants).
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The result of running the golden trace. It is stored as JSON in
/// `tests/golden/flat_drive.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldenTraceResult {
    /// Steps that were run.
    pub steps: u32,
    /// Hash of every step's pose and velocities plus the final snapshot, as 16 hex digits.
    #[serde(rename = "hash")]
    pub hash_hex: String,
    /// Where the car ended up, for a human-readable sanity check.
    pub final_position_m: [f32; 3],
}

/// The scripted driver input for `step`: wait, accelerate, turn left, turn right,
/// brake, then reverse. It is a pure function of the step number.
pub fn scripted_input(step: u32) -> InputFrame {
    let second = step / PHYSICS_HZ;
    let (throttle, steer) = match second {
        0 => (0, 0),          // settle on the suspension
        1..=4 => (127, 0),    // full throttle
        5..=7 => (80, 100),   // throttle and turn left
        8..=10 => (80, -100), // throttle and turn right
        11..=12 => (-127, 0), // brake
        _ => (-90, 30),       // reverse while turning
    };
    InputFrame {
        throttle,
        steer,
        buttons: 0,
    }
}

/// Why the golden trace could not run.
#[derive(Debug, thiserror::Error)]
pub enum GoldenTraceError {
    #[error(transparent)]
    Config(#[from] VehicleConfigError),

    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
}

/// The scripted thruster throttles for `step`, so the trace also covers the nozzle
/// forces: a half-strength lift pulse (enough for a short hop), then S_FL, which
/// pushes the nose right, against the left turn.
pub fn scripted_throttles(step: u32) -> [f32; NOZZLE_COUNT] {
    let second = step / PHYSICS_HZ;
    let mut throttles = [0.0; NOZZLE_COUNT];
    match second {
        2 => throttles[..4].fill(0.5), // all four lift nozzles at half
        6 => throttles[4] = 0.8,       // S_FL, nose-right yaw, during the left turn
        _ => {}
    }
    throttles
}

/// Runs one physics step of the script: like a real controller, the throttles are
/// set only at 60 Hz and the world holds them in between.
pub fn step_scripted(world: &mut World, step: u32) {
    if step % PHYSICS_STEPS_PER_CONTROL_STEP == 0 {
        world.set_controller_throttles(scripted_throttles(step));
    }
    world.step(scripted_input(step));
}

/// Runs the golden trace with the frozen golden car.
pub fn run_golden_trace() -> Result<GoldenTraceResult, GoldenTraceError> {
    let config = VehicleConfig::from_toml_str(GOLDEN_VEHICLE_TOML)?;
    let mut world = World::new_on_flat_ground(&config, DEFAULT_GROUND_FRICTION);

    let mut hash = FNV_OFFSET_BASIS;
    for step in 0..GOLDEN_TRACE_STEPS {
        step_scripted(&mut world, step);
        hash = hash_world_state(hash, &world);
    }
    hash = fnv1a_add_bytes(hash, &world.snapshot()?);

    Ok(GoldenTraceResult {
        steps: GOLDEN_TRACE_STEPS,
        hash_hex: format!("{hash:016x}"),
        final_position_m: world.chassis_pose().position_m,
    })
}

/// Folds the car's position, rotation and velocities into `hash`.
fn hash_world_state(mut hash: u64, world: &World) -> u64 {
    let pose = world.chassis_pose();
    let groups = [
        &pose.position_m[..],
        &pose.rotation_xyzw[..],
        &world.linear_velocity_m_s()[..],
        &world.angular_velocity_rad_s()[..],
    ];
    for values in groups {
        for value in values {
            hash = fnv1a_add_bytes(hash, &value.to_bits().to_le_bytes());
        }
    }
    hash
}

/// One round of the FNV-1a hash: for each byte, xor it in, then multiply.
fn fnv1a_add_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_of_the_letter_a_matches_the_published_value() {
        // Known FNV-1a 64-bit test vector: "a" -> af63dc4c8601ec8c.
        assert_eq!(
            fnv1a_add_bytes(FNV_OFFSET_BASIS, b"a"),
            0xaf63_dc4c_8601_ec8c
        );
    }

    #[test]
    fn the_script_starts_still_and_then_accelerates() {
        assert_eq!(scripted_input(0).throttle, 0);
        assert_eq!(scripted_input(PHYSICS_HZ).throttle, 127);
    }
}
