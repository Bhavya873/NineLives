//! The browser's view of the simulation: the wasm `Game` class (SPEC §11.1).
//!
//! This crate is a thin wrapper. All physics lives in `nl-sim`; here we only turn
//! JavaScript values (strings, byte arrays) into `nl-sim` calls and turn results
//! back into flat `Float32Array`s that three.js can read without extra objects.
//!
//! So far: driving, thrusters, the air tank, modes, Hop and Boost. Controllers, the
//! policy, events and courses are added in M3–M7, following SPEC §11.1.

use nl_sim::conventions::{NOZZLE_COUNT, PHYSICS_DT_S};
use nl_sim::golden_trace::run_golden_trace;
use nl_sim::hop_boost::HopBlockedReason;
use nl_sim::input::{INPUT_FRAME_BYTES, InputFrame};
use nl_sim::modes::{ModeParams, ModesConfig};
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
    /// Air left as a fraction of the tank, 0..1 (always 1 in Arcade).
    AirFraction = 3,
    /// All the air used so far, in kg.
    AirUsedKg = 4,
    /// 1 while a Hop is pushing, else 0.
    HopActive = 5,
    /// Seconds until the next Hop is allowed.
    HopCooldownS = 6,
    /// Why a Hop is not allowed right now: a `HopBlocked` value.
    HopBlocked = 7,
    /// 1 while Boost is held, else 0.
    BoostActive = 8,
    /// 1 when the current mode has unlimited air (the HUD shows ∞), else 0.
    UnlimitedAir = 9,
}

/// Number of values in the HUD array: one more than the last `HudField`.
const HUD_LENGTH: usize = HudField::UnlimitedAir as usize + 1;

/// The values `HudField.HopBlocked` can take (SPEC §7.8).
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum HopBlocked {
    /// A Hop is allowed.
    No = 0,
    /// Fewer than 3 wheels on the ground.
    NotOnGround = 1,
    /// Too soon after the last Hop.
    Cooldown = 2,
    /// Not enough air (Realistic).
    NoAir = 3,
}

/// One running game: a world and the car in it.
#[wasm_bindgen]
pub struct Game {
    world: World,
    modes: ModesConfig,
}

#[wasm_bindgen]
impl Game {
    /// Builds a game on flat ground from the text of `configs/vehicle.toml` and
    /// `configs/modes.toml`, starting in Realistic mode. Throws if a config is
    /// invalid; the message names the bad field.
    #[wasm_bindgen(constructor)]
    pub fn new(vehicle_toml: &str, modes_toml: &str) -> Result<Game, JsError> {
        let config = VehicleConfig::from_toml_str(vehicle_toml)?;
        let modes = ModesConfig::from_toml_str(modes_toml)?;
        let world = World::new_on_flat_ground(&config, modes.realistic, DEFAULT_GROUND_FRICTION);
        Ok(Game { world, modes })
    }

    /// The current mode's name: "realistic", "arcade", or "custom" for anything else
    /// (e.g. a mode sampled in between). Read from the world, so it is still right
    /// after `restore`.
    pub fn mode_name(&self) -> String {
        let mode = self.world.mode();
        if mode == self.modes.realistic {
            "realistic".to_string()
        } else if mode == self.modes.arcade {
            "arcade".to_string()
        } else {
            "custom".to_string()
        }
    }

    /// Switches physics mode: "realistic" or "arcade". Throws on any other name.
    pub fn set_mode(&mut self, mode: &str) -> Result<(), JsError> {
        self.world.set_mode(self.mode_params(mode)?);
        Ok(())
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

    /// How open each of the 10 thruster valves is now (0..1), in nozzle order.
    /// The renderer sizes each flame from these.
    pub fn nozzle_throttles(&self) -> Vec<f32> {
        self.world.effective_throttles().to_vec()
    }

    /// Values for the on-screen display. Read them with the `HudField` indices.
    pub fn hud(&self) -> Vec<f32> {
        let mut values = vec![0.0; HUD_LENGTH];
        values[HudField::SpeedMS as usize] = self.world.forward_speed_m_s();
        values[HudField::TimeS as usize] = self.world.time_s() as f32;
        values[HudField::WheelsTouching as usize] = self.world.wheels_touching_count() as f32;
        values[HudField::AirFraction as usize] = self.world.air_fraction();
        values[HudField::AirUsedKg as usize] = self.world.air_used_kg();
        let hop_boost = self.world.hop_boost();
        values[HudField::HopActive as usize] = one_if(hop_boost.hop_active());
        values[HudField::HopCooldownS as usize] = hop_boost.hop_cooldown_left_s();
        values[HudField::BoostActive as usize] = one_if(hop_boost.boost_active());
        values[HudField::HopBlocked as usize] = hop_blocked_code(self.world.hop_blocked_reason());
        values[HudField::UnlimitedAir as usize] = one_if(self.world.mode().unlimited_air);
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
        Ok(run_golden_trace()?.hash_hex)
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

impl Game {
    /// Looks up a mode by its name in the UI.
    fn mode_params(&self, mode: &str) -> Result<ModeParams, JsError> {
        match mode {
            "realistic" => Ok(self.modes.realistic),
            "arcade" => Ok(self.modes.arcade),
            other => Err(JsError::new(&format!(
                "unknown mode \"{other}\"; use \"realistic\" or \"arcade\""
            ))),
        }
    }
}

/// 1.0 for true, 0.0 for false, for the all-numbers HUD array.
fn one_if(condition: bool) -> f32 {
    if condition { 1.0 } else { 0.0 }
}

/// The `HopBlocked` number for a blocked reason.
fn hop_blocked_code(reason: Option<HopBlockedReason>) -> f32 {
    let code = match reason {
        None => HopBlocked::No,
        Some(HopBlockedReason::NotOnGround) => HopBlocked::NotOnGround,
        Some(HopBlockedReason::Cooldown) => HopBlocked::Cooldown,
        Some(HopBlockedReason::NoAir) => HopBlocked::NoAir,
    };
    code as u8 as f32
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
