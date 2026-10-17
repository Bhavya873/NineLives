//! One training or eval episode (SPEC §9.5): a fresh world started from a scenario, the
//! rules for when it ends, and the per-episode numbers SPEC §10 reports. The Rust eval
//! runner, the M5 Python binding and the browser replay all step episodes through this,
//! so "success", "crash" and "time limit" mean the same thing everywhere.

use serde::Serialize;

use crate::car_state::CarState;
use crate::conventions::{NOZZLE_COUNT, PHYSICS_HZ};
use crate::events::{CrashCause, SimEvent};
use crate::input::InputFrame;
use crate::modes::ModeParams;
use crate::scenarios::{EpisodeConfig, ScenariosConfig, Stage, start_episode};
use crate::vehicle_config::VehicleConfig;
use crate::world::{DEFAULT_GROUND_FRICTION, PlaceCarError, World};

/// Where an episode stands after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpisodeStatus {
    /// Still going.
    Running,
    /// A save, then `success_hold_s` with no crash (SPEC §9.5).
    Succeeded,
    /// The car crashed; this ends the episode.
    Crashed(CrashCause),
    /// The time limit ran out first.
    TimedOut,
}

/// What happened in one episode (SPEC §10's per-episode row).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EpisodeResult {
    /// The seed the episode was started from.
    pub seed: u64,
    /// A save, then `success_hold_s` with no crash.
    pub success: bool,
    /// Save events seen.
    pub saves: u32,
    /// Why it crashed, if it did (a crash ends the episode, so there is at most one).
    pub crash_cause: Option<CrashCause>,
    /// Air the thrusters used, in kg.
    pub air_used_kg: f32,
    /// The largest tilt from upright, in degrees.
    pub peak_tilt_deg: f32,
    /// Spin size just before the first touchdown, if the car landed, in rad/s.
    pub touchdown_spin_rad_s: Option<f32>,
    /// Fall speed just before the first touchdown, if the car landed, in m/s.
    pub touchdown_fall_m_s: Option<f32>,
    /// How long the episode ran, in seconds.
    pub duration_s: f32,
}

/// A running episode: the world plus the numbers gathered so far.
pub struct Episode {
    world: World,
    rules: EpisodeConfig,
    limit_steps: u64,
    steps: u64,
    result: EpisodeResult,
    first_save_s: Option<f32>,
    peak_tilt_rad: f32,
}

impl Episode {
    /// A fresh flat-ground world with the car placed for `stage` from `seed`.
    pub fn start(
        vehicle: &VehicleConfig,
        mode: ModeParams,
        scenarios: &ScenariosConfig,
        stage: Stage,
        seed: u64,
    ) -> Result<Episode, PlaceCarError> {
        let mut world = World::new_on_flat_ground(vehicle, mode, DEFAULT_GROUND_FRICTION, seed);
        start_episode(&mut world, stage, scenarios, seed)?;
        let rules = scenarios.episode;
        Ok(Episode {
            world,
            rules,
            limit_steps: (rules.time_limit_s * PHYSICS_HZ as f32).round() as u64,
            steps: 0,
            result: EpisodeResult {
                seed,
                success: false,
                saves: 0,
                crash_cause: None,
                air_used_kg: 0.0,
                peak_tilt_deg: 0.0,
                touchdown_spin_rad_s: None,
                touchdown_fall_m_s: None,
                duration_s: 0.0,
            },
            first_save_s: None,
            peak_tilt_rad: 0.0,
        })
    }

    /// The world, for controllers (`car_state`, `is_control_step`) and rendering.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// One physics step with the controller's latest throttles (0..1, nozzle order).
    /// Does nothing once the episode has ended.
    pub fn step(&mut self, throttles: [f32; NOZZLE_COUNT]) -> EpisodeStatus {
        if self.status() != EpisodeStatus::Running {
            return self.status();
        }
        let before = self.world.car_state();
        self.world.set_controller_throttles(throttles);
        self.world.step(InputFrame::default());
        self.steps += 1;
        self.peak_tilt_rad = self.peak_tilt_rad.max(self.world.tilt_rad());
        for event in self.world.drain_events() {
            self.record(event, &before);
        }
        self.status()
    }

    /// SPEC §9.5: a crash ends the episode, and so does a save held long enough.
    pub fn status(&self) -> EpisodeStatus {
        if let Some(cause) = self.result.crash_cause {
            return EpisodeStatus::Crashed(cause);
        }
        if self.save_held_long_enough() {
            return EpisodeStatus::Succeeded;
        }
        if self.steps >= self.limit_steps {
            return EpisodeStatus::TimedOut;
        }
        EpisodeStatus::Running
    }

    /// The per-episode row so far (final once `status` is not `Running`).
    pub fn result(&self) -> EpisodeResult {
        EpisodeResult {
            success: self.status() == EpisodeStatus::Succeeded,
            air_used_kg: self.world.air_used_kg(),
            peak_tilt_deg: self.peak_tilt_rad.to_degrees(),
            duration_s: self.world.time_s() as f32,
            ..self.result.clone()
        }
    }

    /// Adds one event to the tally. `before` is the car just before this step, so a
    /// touchdown records the spin and fall speed it landed with.
    fn record(&mut self, event: SimEvent, before: &CarState) {
        match event {
            SimEvent::AirborneEnd if self.result.touchdown_spin_rad_s.is_none() => {
                let [x, y, z] = before.angular_velocity_body_rad_s;
                self.result.touchdown_spin_rad_s = Some((x * x + y * y + z * z).sqrt());
                self.result.touchdown_fall_m_s = Some(before.falling_speed_m_s());
            }
            SimEvent::Save { .. } => {
                self.result.saves += 1;
                let now_s = self.world.time_s() as f32;
                self.first_save_s.get_or_insert(now_s);
            }
            SimEvent::Crash { cause } => self.result.crash_cause = Some(cause),
            _ => {}
        }
    }

    fn save_held_long_enough(&self) -> bool {
        match self.first_save_s {
            Some(save_s) => self.world.time_s() as f32 - save_s >= self.rules.success_hold_s,
            None => false,
        }
    }
}
