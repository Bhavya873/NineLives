//! The golden trace: a fixed, scripted drive used to prove determinism (SPEC §7.10).
//!
//! The same 2000 steps of scripted input are run natively and in wasm. After every
//! step the car's pose and velocities are folded into a 64-bit FNV-1a hash of their
//! exact bits. If any bit differs anywhere, the final hash differs. The expected hash
//! is stored in `crates/nl-sim/tests/golden/flat_drive.json` and checked in CI.

use crate::conventions::PHYSICS_HZ;
use crate::input::InputFrame;
use crate::vehicle_config::{VehicleConfig, VehicleConfigError};
use crate::world::World;

/// Number of physics steps in the golden trace (SPEC §7.10).
pub const GOLDEN_TRACE_STEPS: u32 = 2000;

/// Ground friction used for the golden trace.
const GOLDEN_TRACE_GROUND_FRICTION: f32 = 1.0;

/// FNV-1a 64-bit starting value and multiplier (the standard constants).
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The result of running the golden trace.
#[derive(Debug, Clone, PartialEq)]
pub struct GoldenTraceResult {
    /// Steps that were run.
    pub steps: u32,
    /// Hash of every step's pose and velocities, as 16 hex digits.
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

/// Runs the golden trace with the given vehicle config text.
pub fn run_golden_trace(vehicle_toml: &str) -> Result<GoldenTraceResult, VehicleConfigError> {
    let config = VehicleConfig::from_toml_str(vehicle_toml)?;
    let mut world = World::new_on_flat_ground(&config, GOLDEN_TRACE_GROUND_FRICTION);

    let mut hash = FNV_OFFSET_BASIS;
    for step in 0..GOLDEN_TRACE_STEPS {
        world.step(scripted_input(step));
        hash = hash_world_state(hash, &world);
    }

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
