//! The browser's view of the simulation: the wasm `Game` class (SPEC §11.1).
//!
//! This crate is a thin wrapper. All physics lives in `nl-sim`; here we only turn
//! JavaScript values (strings, byte arrays) into `nl-sim` calls and turn results
//! back into flat `Float32Array`s that three.js can read without extra objects.
//!
//! M2 provides driving only. Modes, controllers, the policy, the HUD fields for air
//! and events, and courses are added in M3–M7, following SPEC §11.1.

use nl_sim::conventions::{NOZZLE_COUNT, PHYSICS_DT_S};
use nl_sim::golden_trace::run_golden_trace;
use nl_sim::input::{INPUT_FRAME_BYTES, InputFrame};
use nl_sim::vehicle_config::VehicleConfig;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, Pose, SNAPSHOT_FORMAT_VERSION, World};
use wasm_bindgen::prelude::*;

/// Number of floats in one pose: position x, y, z then quaternion x, y, z, w.
const FLOATS_PER_POSE: usize = 7;

/// Where each value sits in the array returned by `Game::hud`. JavaScript sees this
/// as an object, so it can write `hud[HudField.SpeedMS]` instead of a bare number.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum HudField {
    /// Forward speed in m/s (negative when reversing).
    SpeedMS = 0,
    /// Simulated time since the game started, in seconds.
    TimeS = 1,
    /// How many wheels touch the ground (0 to 4).
    WheelsTouching = 2,
}

/// Number of values in the HUD array. Keep it one more than the last `HudField`.
const HUD_LENGTH: usize = 3;

/// One running game: a world and the car in it.
#[wasm_bindgen]
pub struct Game {
    world: World,
}

#[wasm_bindgen]
impl Game {
    /// Builds a game on flat ground from the text of `configs/vehicle.toml`.
    /// Throws if the config is invalid; the message names the bad field.
    #[wasm_bindgen(constructor)]
    pub fn new(vehicle_toml: &str) -> Result<Game, JsError> {
        let config = VehicleConfig::from_toml_str(vehicle_toml)?;
        let world = World::new_on_flat_ground(&config, DEFAULT_GROUND_FRICTION);
        Ok(Game { world })
    }

    /// Advances exactly one physics step (1/120 s). `input` is a packed 4-byte
    /// `InputFrame`: throttle (i8), steer (i8), buttons (u16, little-endian).
    pub fn tick(&mut self, input: &[u8]) -> Result<(), JsError> {
        let bytes: [u8; INPUT_FRAME_BYTES] = input.try_into().map_err(|_| {
            JsError::new(&format!(
                "tick() needs {INPUT_FRAME_BYTES} input bytes, got {}",
                input.len()
            ))
        })?;
        self.world.step(InputFrame::from_bytes(bytes));
        Ok(())
    }

    /// The car body's pose: [x, y, z, qx, qy, qz, qw].
    pub fn chassis_pose(&self) -> Vec<f32> {
        flatten_poses(&[self.world.chassis_pose()])
    }

    /// The four wheel poses in order FL, FR, RL, RR, each [x, y, z, qx, qy, qz, qw].
    pub fn wheel_poses(&self) -> Vec<f32> {
        flatten_poses(&self.world.wheel_poses())
    }

    /// Values for the on-screen display. Read them with the `HudField` indices.
    pub fn hud(&self) -> Vec<f32> {
        let mut values = vec![0.0; HUD_LENGTH];
        values[HudField::SpeedMS as usize] = self.world.forward_speed_m_s();
        values[HudField::TimeS as usize] = self.world.time_s() as f32;
        values[HudField::WheelsTouching as usize] = self
            .world
            .wheels_touching()
            .iter()
            .filter(|touching| **touching)
            .count() as f32;
        values
    }

    /// Saves the whole game state as bytes, for checkpoints and respawning.
    pub fn snapshot(&self) -> Result<Vec<u8>, JsError> {
        Ok(self.world.snapshot()?)
    }

    /// Loads a state saved by `snapshot`. Throws on bytes from another build.
    pub fn restore(&mut self, snapshot: &[u8]) -> Result<(), JsError> {
        self.world.restore(snapshot)?;
        Ok(())
    }

    /// Identifies this build, so saved ghosts from an incompatible build are ignored.
    /// For now it is the crate version and snapshot format; M8 adds the physics hash.
    pub fn build_hash() -> String {
        format!(
            "nl-wasm-{}-snapshot-v{}",
            env!("CARGO_PKG_VERSION"),
            SNAPSHOT_FORMAT_VERSION
        )
    }

    /// Runs the 2000-step golden trace and returns its hash, so a test can check
    /// that wasm gives bit-identical physics to native (SPEC §7.10).
    pub fn golden_trace_hash() -> Result<String, JsError> {
        let result = run_golden_trace().map_err(|error| JsError::new(&error.to_string()))?;
        Ok(result.hash_hex)
    }

    /// The fixed physics step length in seconds (1/120).
    pub fn physics_dt_s() -> f32 {
        PHYSICS_DT_S
    }

    /// Number of floats per pose in `chassis_pose` and `wheel_poses` (7).
    pub fn floats_per_pose() -> usize {
        FLOATS_PER_POSE
    }

    /// Number of thruster nozzles (10).
    pub fn nozzle_count() -> usize {
        NOZZLE_COUNT
    }
}

/// Writes poses one after another as [x, y, z, qx, qy, qz, qw, x, y, z, ...].
fn flatten_poses(poses: &[Pose]) -> Vec<f32> {
    let mut values = Vec::with_capacity(poses.len() * FLOATS_PER_POSE);
    for pose in poses {
        values.extend_from_slice(&pose.position_m);
        values.extend_from_slice(&pose.rotation_xyzw);
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poses_are_flattened_position_first_then_quaternion() {
        let pose = Pose {
            position_m: [1.0, 2.0, 3.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        };

        let values = flatten_poses(&[pose, pose]);

        assert_eq!(values.len(), 14);
        assert_eq!(&values[0..7], &[1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 1.0]);
    }
}
