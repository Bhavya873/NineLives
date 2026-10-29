//! The browser's view of the simulation: the wasm `Game` class (SPEC §11.1).
//!
//! This crate is a thin wrapper. All physics lives in `nl-sim`; here we only turn
//! JavaScript values (strings, byte arrays) into `nl-sim` calls and turn results
//! back into flat `Float32Array`s that three.js can read without extra objects.
//!
//! So far: driving, thrusters, the air tank, modes, Hop and Boost, events, respawn and
//! the Off, PD, AI and (debug-only) Manual controllers, and courses (`course.rs`, M7).

use nl_control::{ControlCtx, Controller, ControllerChoices, ControllerKind};
use std::sync::Arc;

use nl_policy::Policy;
use nl_sim::conventions::{INVERTED_TILT_DEG, NOZZLE_COUNT, PHYSICS_DT_S};
use nl_sim::events::{CrashCause, SimEvent};
use nl_sim::golden_trace::run_golden_trace;
use nl_sim::hop_boost::HopBlockedReason;
use nl_sim::input::{
    BUTTON_BOOST, BUTTON_CHAOS, BUTTON_HOP, BUTTON_RESET, FIRST_MANUAL_NOZZLE_BIT,
    INPUT_FRAME_BYTES, InputFrame,
};
use nl_sim::modes::{ModeParams, ModesConfig};
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig, Stage, start_episode};
use nl_sim::vehicle_config::VehicleConfig;
use nl_sim::wind::WindLevel;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, Pose, SNAPSHOT_FORMAT_VERSION, World};
use wasm_bindgen::prelude::*;

mod course;
mod crowd;
mod eval;

/// Number of floats in one pose: position x, y, z then quaternion x, y, z, w.
pub(crate) const FLOATS_PER_POSE: usize = 7;

/// Number of floats per nozzle in `Game::nozzle_layout`: position x, y, z, then the
/// exhaust direction x, y, z.
const FLOATS_PER_NOZZLE: usize = 6;

// Reserved numbers (docs/contracts/events-hud.md, frozen at Gate 0). Do not reuse them
// for anything else; each PR below adds its own variants.
//   HudField  23 PeakGLoad, 24 StuntPhase, 25 StuntCode, 26 ApproachHint,
//             27 PolicyOutOfEnvelope.
//   EventKind 11 StuntStart, 12 StuntLanded, 13 StuntRefused, 14 StuntAborted
//             (15 Scrape and 16 TooSlowFallback exist since G2).
// The Save event will gain a 5th float, `peak_g` (FLOATS_PER_EVENT goes 4 -> 5).

/// Where each value sits in the array returned by `Game::hud`. JavaScript sees this
/// as an object, so it can write `hud[HudField.SpeedMS]` instead of a bare number.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum HudField {
    /// Forward speed in m/s (negative when reversing; 0 while the wreck is frozen).
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
    /// The current mode: a `ModeCode` value. Read from the world, so it is still
    /// right after `restore`.
    Mode = 10,
    /// Saves since the game started (counted by `drain_events`).
    Saves = 11,
    /// Crashes since the game started, shown as "Lives used" (counted by `drain_events`).
    Crashes = 12,
    /// 1 from a crash until the respawn (the wreck is frozen meanwhile), else 0.
    Crashed = 13,
    /// 1 while the tank is so low that thrust fades ("NO AIR"), else 0.
    OutOfAir = 14,
    /// Which controller fires the thrusters: a `ControllerCode` value.
    Controller = 15,
    /// Course checkpoints behind the car (0 on flat ground).
    CheckpointsReached = 16,
    /// Checkpoints on the course (0 on flat ground).
    CheckpointCount = 17,
    /// 1 once the car has crossed the course's finish line, else 0.
    Finished = 18,
    /// Course time at the finish, in s (0 until then).
    FinishTimeS = 19,
    /// Passenger g-load at the driver's seat, in g (SPEC §9.4): 1 sitting still; the
    /// screen darkens above 3 g.
    GLoad = 20,
    /// The wind now (mean plus gust) along world +X, in m/s.
    WindXMS = 21,
    /// The wind now (mean plus gust) along world +Z, in m/s.
    WindZMS = 22,
}

/// Number of values in the HUD array: one more than the last `HudField`.
const HUD_LENGTH: usize = HudField::WindZMS as usize + 1;

/// The values `HudField.Controller` can take (SPEC §8).
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ControllerCode {
    /// No thrusters (Hop and Boost still work).
    Off = 0,
    /// Debug: keys 1–0 fire nozzles 0–9.
    Manual = 1,
    /// The PD baseline (SPEC §8).
    Pd = 2,
    /// The trained policy (SPEC §8).
    Ai = 3,
    /// Hybrid with the AI flying the car (DECISIONS 2026-10-21).
    HybridAi = 4,
    /// Hybrid with PD in charge: the car is on its wheels.
    HybridPd = 5,
}

/// The values `HudField.Mode` can take.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum ModeCode {
    Realistic = 0,
    Arcade = 1,
    /// Anything else, e.g. a mode sampled between the two for training.
    Custom = 2,
}

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
    /// The car, kept to rebuild the world for a course (`load_course`).
    vehicle: VehicleConfig,
    /// The seed the game started with; a rebuilt world reuses it.
    seed: u64,
    modes: ModesConfig,
    /// Where each nozzle sits and which way its exhaust points (see `nozzle_layout`).
    nozzle_layout: Vec<f32>,
    /// What fires the thrusters, asked at 60 Hz in `tick`.
    controller: ControllerKind,
    /// What the named controllers are built from: PD for this car (from
    /// `configs/controllers.toml`) and, from M6, the loaded policy.
    choices: ControllerChoices,
    /// Starts for the drop test: the repo's scenarios.toml, built in (the game passes no
    /// scenarios of its own).
    scenarios: ScenariosConfig,
    /// The wind setting (SPEC §7.2), kept so a rebuilt world (`load_course`) gets it too.
    wind_level: WindLevel,
    /// Buttons pressed on any tick since the last control step, so a quick tap of a
    /// nozzle key between two control steps still reaches the controller.
    buttons_since_control_step: u16,
    /// Saves and crashes since the game started, counted as events are drained.
    saves: u32,
    crashes: u32,
}

/// Number of floats per event in `Game::drain_events`.
const FLOATS_PER_EVENT: usize = 4;

/// What happened, the first of the 4 floats of each event from `drain_events`.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum EventKind {
    /// The last wheel left the ground.
    AirborneStart = 1,
    /// A wheel touched the ground again.
    AirborneEnd = 2,
    /// The car was knocked off balance.
    UpsetStart = 3,
    /// Followed by: duration in s, air used in kg, peak tilt in rad.
    Save = 4,
    /// Followed by: a `CrashCode`.
    Crash = 5,
    /// The car is back at the last checkpoint (after a crash or Reset).
    Respawn = 6,
    /// Followed by: the checkpoint's number (0 first).
    CheckpointReached = 7,
    /// Followed by: the time since the world started, in s.
    Finished = 8,
    /// Followed by: the kick plate's piece index in the course file.
    Kicked = 9,
    /// A wheel bottomed out at touchdown (not a crash). Followed by: the largest
    /// suspension compression (0..1), the fall speed just before touchdown in m/s.
    HardLanding = 10,
    /// A gentle underbody touch, not a crash (docs/contracts/crash-scrape.md). Followed
    /// by: the normal speed into the surface in m/s. Numbers 11 to 14 are reserved for the
    /// stunt events (docs/contracts/events-hud.md).
    Scrape = 15,
    /// The car entered a piece below its minimum completion speed and takes its safe
    /// fallback. Followed by: the piece's index in the course file, the entry speed in m/s.
    TooSlowFallback = 16,
}

/// Why the car crashed, the second float of a `Crash` event.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum CrashCode {
    /// The body or roof hit something too fast.
    HardImpact = 1,
    /// The roof touched something while upside down.
    UpsideDown = 2,
    /// Tipped over for too long.
    Stuck = 3,
    /// Fell out of the world.
    OutOfBounds = 4,
}

#[wasm_bindgen]
impl Game {
    /// Builds a game on flat ground from the text of `configs/vehicle.toml`,
    /// `configs/modes.toml` and `configs/controllers.toml`, starting in Realistic mode
    /// with the PD controller. `seed` drives the random
    /// Chaos knocks. A crash respawns by itself after `[events] respawn_delay_s`.
    /// Throws if a config is invalid; the message names the bad field.
    #[wasm_bindgen(constructor)]
    pub fn new(
        vehicle_toml: &str,
        modes_toml: &str,
        controllers_toml: &str,
        seed: u64,
    ) -> Result<Game, JsError> {
        let config = VehicleConfig::from_toml_str(vehicle_toml)?;
        let modes = ModesConfig::from_toml_str(modes_toml)?;
        let mut world =
            World::new_on_flat_ground(&config, modes.realistic, DEFAULT_GROUND_FRICTION, seed);
        world.enable_auto_respawn(config.events.respawn_delay_s);
        let choices =
            ControllerChoices::from_configs(&config, modes.realistic, controllers_toml, None)?;
        Ok(Game {
            world,
            vehicle: config.clone(),
            seed,
            modes,
            nozzle_layout: flatten_nozzle_layout(&config),
            // PD starts in charge, so a new player sees the car land itself (SPEC §11.3).
            controller: ControllerKind::Pd(Box::new(choices.pd.clone())),
            choices,
            scenarios: ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML)?,
            wind_level: WindLevel::Off,
            buttons_since_control_step: 0,
            saves: 0,
            crashes: 0,
        })
    }

    /// Switches physics mode: "realistic" or "arcade". Throws on any other name.
    pub fn set_mode(&mut self, mode: &str) -> Result<(), JsError> {
        self.world.set_mode(self.mode_params(mode)?);
        Ok(())
    }

    /// Sets the wind: "off", "breeze" or "storm" (SPEC §7.2, levels from `[wind]` in
    /// scenarios.toml), blowing in a direction drawn from the game's seed, with gusts.
    /// Throws on any other name. Loading a course keeps the setting.
    pub fn set_wind_level(&mut self, level: &str) -> Result<(), JsError> {
        let level = WindLevel::from_name(level).ok_or_else(|| {
            JsError::new(&format!(
                "set_wind_level: unknown level '{level}'; use off, breeze or storm"
            ))
        })?;
        self.wind_level = level;
        self.apply_wind_level();
        Ok(())
    }

    /// Chooses what fires the thrusters by name: "off", "pd", "ai", "manual" or
    /// "hybrid". Throws on any other name, or on "ai" or "hybrid" before a policy is
    /// loaded (`load_policy`). It takes
    /// over at the next control step.
    pub fn set_controller(&mut self, kind: &str) -> Result<(), JsError> {
        self.use_controller(ControllerKind::from_name(kind, &self.choices)?);
        Ok(())
    }

    /// Loads the trained policy (the text of `models/policy.json`) for the "ai" and
    /// "hybrid" controllers. Throws, naming the problem, if the file is not a policy or was
    /// trained on another observation layout (its schema hash differs, SPEC §9.12). A
    /// running AI or Hybrid switches to the new policy straight away.
    pub fn load_policy(&mut self, json: &str) -> Result<(), JsError> {
        self.choices.policy = Some(Arc::new(Policy::from_json(json)?));
        if matches!(
            self.controller,
            ControllerKind::Ai(_) | ControllerKind::Hybrid(_)
        ) {
            let name = self.controller.name();
            self.use_controller(ControllerKind::from_name(name, &self.choices)?);
        }
        Ok(())
    }

    /// True once a policy is loaded, so "ai" can be chosen.
    pub fn has_policy(&self) -> bool {
        self.choices.policy.is_some()
    }

    /// A drop test (SPEC §9.6 Stage A, full difficulty): puts the car in the air above
    /// the arena centre in a random attitude, spin and speed drawn from `seed`, with a
    /// full tank, exactly as a training episode starts. The respawn point is unchanged.
    pub fn drop_test(&mut self, seed: u64) -> Result<(), JsError> {
        start_episode(&mut self.world, Stage::A, &self.scenarios, seed)?;
        self.controller.reset(seed);
        // Taps from before the drop must not reach its first control step.
        self.buttons_since_control_step = 0;
        Ok(())
    }

    /// Switches to the next controller in the T-key cycle (SPEC §11.3).
    pub fn cycle_controller(&mut self) {
        let next = self.controller.next(&self.choices);
        self.use_controller(next);
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
        let input = InputFrame::from_bytes(bytes);
        // Controllers run at 60 Hz; the world holds their throttles in between (SPEC §7.1).
        self.buttons_since_control_step |= input.buttons;
        if self.world.is_crashed() {
            // The wreck is frozen until it respawns: nothing to control, and the
            // controller should start fresh on the respawned car.
            self.controller.reset(0);
        } else if self.world.is_control_step() {
            let controller_input = InputFrame {
                buttons: self.buttons_since_control_step,
                ..input
            };
            self.buttons_since_control_step = 0;
            let ctx = ControlCtx::for_world(&self.world, controller_input);
            let throttles = self.controller.act(&ctx);
            self.world.set_controller_throttles(throttles);
        }
        self.world.step(input);
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

    /// How much gas each of the 10 nozzles is blowing now (0..1), in nozzle order:
    /// valve opening times the tank gate. The renderer sizes each flame from these.
    pub fn nozzle_throttles(&self) -> Vec<f32> {
        self.world.flame_fractions().to_vec()
    }

    /// Where the renderer draws each nozzle and its flame, in nozzle order, 6 floats
    /// each in the car's body frame (+X forward, +Y up, +Z right): position in metres,
    /// then the unit exhaust direction (opposite to the push, SPEC §7.4).
    pub fn nozzle_layout(&self) -> Vec<f32> {
        self.nozzle_layout.clone()
    }

    /// Values for the on-screen display. Read them with the `HudField` indices.
    pub fn hud(&self) -> Vec<f32> {
        let mut values = vec![0.0; HUD_LENGTH];
        let crashed = self.world.is_crashed();
        // The frozen wreck keeps its last velocity; report it as stopped.
        let speed_m_s = if crashed {
            0.0
        } else {
            self.world.forward_speed_m_s()
        };
        values[HudField::SpeedMS as usize] = speed_m_s;
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
        values[HudField::Mode as usize] = self.mode_code() as u8 as f32;
        values[HudField::Saves as usize] = self.saves as f32;
        values[HudField::Crashes as usize] = self.crashes as f32;
        values[HudField::Crashed as usize] = one_if(crashed);
        values[HudField::OutOfAir as usize] = one_if(self.world.is_out_of_air());
        values[HudField::Controller as usize] = controller_code(&self.controller) as u8 as f32;
        values[HudField::CheckpointsReached as usize] = self.world.checkpoints_reached() as f32;
        values[HudField::CheckpointCount as usize] = self.world.checkpoint_count() as f32;
        values[HudField::Finished as usize] = one_if(self.world.is_finished());
        values[HudField::FinishTimeS as usize] = self.world.finish_time_s().unwrap_or(0.0);
        values[HudField::GLoad as usize] = self.world.g_load();
        let [wind_x_m_s, _, wind_z_m_s] = self.world.wind_m_s();
        values[HudField::WindXMS as usize] = wind_x_m_s;
        values[HudField::WindZMS as usize] = wind_z_m_s;
        values
    }

    /// Saves the whole game state as bytes.
    pub fn snapshot(&self) -> Result<Vec<u8>, JsError> {
        Ok(self.world.snapshot()?)
    }

    /// Loads a state saved by `snapshot`. Throws on bytes from another build.
    pub fn restore(&mut self, snapshot: &[u8]) -> Result<(), JsError> {
        self.world.restore(snapshot)?;
        Ok(())
    }

    /// Makes the car's current pose and motion the place it respawns. A crash
    /// respawns there by itself after the delay; the Reset button does it at once.
    pub fn set_checkpoint(&mut self) {
        self.world.set_checkpoint();
    }

    /// True from a crash until the respawn (the wreck stays frozen meanwhile).
    pub fn is_crashed(&self) -> bool {
        self.world.is_crashed()
    }

    /// Events since the last call, 4 floats each: an `EventKind` then up to three
    /// values (see `EventKind`), unused ones 0. Also counts saves and crashes for `hud`.
    pub fn drain_events(&mut self) -> Vec<f32> {
        let mut values = Vec::new();
        for event in self.world.drain_events() {
            match event {
                SimEvent::Save { .. } => self.saves += 1,
                SimEvent::Crash { .. } => self.crashes += 1,
                _ => {}
            }
            values.extend_from_slice(&event_floats(event));
        }
        values
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

    /// The input-frame button bit for Hop (the web checks its copy against this).
    pub fn button_hop() -> u16 {
        BUTTON_HOP
    }

    /// The input-frame button bit for Boost.
    pub fn button_boost() -> u16 {
        BUTTON_BOOST
    }

    /// The input-frame button bit for Chaos.
    pub fn button_chaos() -> u16 {
        BUTTON_CHAOS
    }

    /// The input-frame button bit for Reset (respawn at the last checkpoint).
    pub fn button_reset() -> u16 {
        BUTTON_RESET
    }

    /// Number of floats per event in `drain_events` (4).
    pub fn floats_per_event() -> usize {
        FLOATS_PER_EVENT
    }

    /// The tilt, in degrees, past which the car counts as flipped (the eval's
    /// inversion rate uses the same mark), so the HUD can call a save a flip save.
    pub fn inverted_tilt_deg() -> f32 {
        INVERTED_TILT_DEG
    }

    /// Bit of the first manual nozzle button; nozzle `i` uses this bit + `i`.
    pub fn first_manual_nozzle_bit() -> u32 {
        FIRST_MANUAL_NOZZLE_BIT
    }

    /// Number of floats per nozzle in `nozzle_layout` (6).
    pub fn floats_per_nozzle() -> usize {
        FLOATS_PER_NOZZLE
    }

    /// Number of thruster nozzles (10).
    pub fn nozzle_count() -> usize {
        NOZZLE_COUNT
    }
}

impl Game {
    /// Starts `controller` fresh. The throttles the old one asked for are dropped now,
    /// so they do not linger until the new one's first control step.
    fn use_controller(&mut self, mut controller: ControllerKind) {
        controller.reset(0);
        self.controller = controller;
        self.world.set_controller_throttles([0.0; NOZZLE_COUNT]);
    }

    /// Which preset the world's current mode is.
    fn mode_code(&self) -> ModeCode {
        let mode = self.world.mode();
        if mode == self.modes.realistic {
            ModeCode::Realistic
        } else if mode == self.modes.arcade {
            ModeCode::Arcade
        } else {
            ModeCode::Custom
        }
    }

    /// Looks up a mode by its name in the UI.
    fn mode_params(&self, mode: &str) -> Result<ModeParams, JsError> {
        self.modes.by_name(mode).ok_or_else(|| {
            JsError::new(&format!(
                "unknown mode \"{mode}\"; use \"realistic\" or \"arcade\""
            ))
        })
    }
}

/// One event as 4 numbers for JavaScript (see `EventKind`).
fn event_floats(event: SimEvent) -> [f32; FLOATS_PER_EVENT] {
    match event {
        SimEvent::AirborneStart => [kind_code(EventKind::AirborneStart), 0.0, 0.0, 0.0],
        SimEvent::AirborneEnd => [kind_code(EventKind::AirborneEnd), 0.0, 0.0, 0.0],
        SimEvent::UpsetStart => [kind_code(EventKind::UpsetStart), 0.0, 0.0, 0.0],
        SimEvent::Save {
            duration_s,
            air_used_kg,
            peak_tilt_rad,
            ..
        } => [
            kind_code(EventKind::Save),
            duration_s,
            air_used_kg,
            peak_tilt_rad,
        ],
        SimEvent::Crash { cause } => [kind_code(EventKind::Crash), crash_code(cause), 0.0, 0.0],
        SimEvent::Respawn => [kind_code(EventKind::Respawn), 0.0, 0.0, 0.0],
        SimEvent::CheckpointReached { number } => [
            kind_code(EventKind::CheckpointReached),
            number as f32,
            0.0,
            0.0,
        ],
        SimEvent::Finished { time_s } => [kind_code(EventKind::Finished), time_s, 0.0, 0.0],
        SimEvent::Kicked { piece } => [kind_code(EventKind::Kicked), piece as f32, 0.0, 0.0],
        SimEvent::Scrape { normal_speed_m_s } => {
            [kind_code(EventKind::Scrape), normal_speed_m_s, 0.0, 0.0]
        }
        SimEvent::TooSlowFallback {
            piece,
            entry_speed_m_s,
        } => [
            kind_code(EventKind::TooSlowFallback),
            piece as f32,
            entry_speed_m_s,
            0.0,
        ],
        SimEvent::HardLanding {
            compression,
            fall_m_s,
        } => [
            kind_code(EventKind::HardLanding),
            compression,
            fall_m_s,
            0.0,
        ],
    }
}

/// An `EventKind` as the number JavaScript sees.
fn kind_code(kind: EventKind) -> f32 {
    kind as u8 as f32
}

/// A crash cause as its `CrashCode` number.
fn crash_code(cause: CrashCause) -> f32 {
    let code = match cause {
        CrashCause::HardImpact => CrashCode::HardImpact,
        CrashCause::UpsideDown => CrashCode::UpsideDown,
        CrashCause::Stuck => CrashCode::Stuck,
        CrashCause::OutOfBounds => CrashCode::OutOfBounds,
    };
    code as u8 as f32
}

/// The HUD code for a controller.
fn controller_code(controller: &ControllerKind) -> ControllerCode {
    match controller {
        ControllerKind::Off(_) => ControllerCode::Off,
        ControllerKind::Pd(_) => ControllerCode::Pd,
        ControllerKind::Ai(_) => ControllerCode::Ai,
        ControllerKind::Manual(_) => ControllerCode::Manual,
        ControllerKind::Hybrid(hybrid) if hybrid.ai_in_control() => ControllerCode::HybridAi,
        ControllerKind::Hybrid(_) => ControllerCode::HybridPd,
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

/// Each nozzle's position then exhaust direction, one after another.
fn flatten_nozzle_layout(config: &VehicleConfig) -> Vec<f32> {
    let mut values = Vec::with_capacity(NOZZLE_COUNT * FLOATS_PER_NOZZLE);
    for nozzle in &config.thrusters.nozzles {
        values.extend_from_slice(&nozzle.position_m);
        values.extend_from_slice(&nozzle.exhaust_direction());
    }
    values
}

/// Writes poses one after another as [x, y, z, qx, qy, qz, qw, x, y, z, ...].
pub(crate) fn flatten_poses(poses: &[Pose]) -> Vec<f32> {
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
