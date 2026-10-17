//! One training or eval episode (SPEC §9.5): a fresh world started from a scenario, the
//! rules for when it ends, and the per-episode numbers SPEC §10 reports. The Rust eval
//! runner, the M5 Python binding and the browser replay all step episodes through this,
//! so "success", "crash" and "time limit" mean the same thing everywhere. It also applies
//! the episode's domain randomization (SPEC §9.8): the varied car and world, actuation
//! latency and observation noise, so training and eval randomize the same way.

use std::collections::VecDeque;

use rand_chacha::ChaCha8Rng;
use serde::Serialize;

use crate::car_state::{CarState, Touchdown};
use crate::conventions::{NOZZLE_COUNT, PHYSICS_HZ, WHEEL_COUNT};
use crate::events::{CrashCause, SimEvent};
use crate::input::InputFrame;
use crate::obs::{Observation, PREV_ACTION_INDEX, observe};
use crate::randomization::{EpisodeVariation, ObsNoise, add_state_noise, noise_rng};
use crate::scenarios::{EpisodeConfig, ScenariosConfig, Stage, start_episode};
use crate::world::{PlaceCarError, World};

/// Where an episode stands after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpisodeStatus {
    /// Still going.
    Running,
    /// All four wheels down for `success_hold_s`, unbroken, with no crash (SPEC §9.5).
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
    /// All four wheels down for `success_hold_s`, unbroken, with no crash.
    pub success: bool,
    /// Save events seen (a recovery from an upset, SPEC §7.9).
    pub saves: u32,
    /// Why it crashed, if it did (a crash ends the episode, so there is at most one).
    pub crash_cause: Option<CrashCause>,
    /// Air the thrusters used over the whole episode, in kg.
    pub air_used_kg: f32,
    /// Air used during the upsets that ended in a save, in kg (from the Save events).
    pub air_used_in_saves_kg: f32,
    /// The largest tilt from upright, in degrees.
    pub peak_tilt_deg: f32,
    /// Spin size just before the car first touched the ground (wheels, body or roof),
    /// in rad/s; None if it never did.
    pub touchdown_spin_rad_s: Option<f32>,
    /// Fall speed just before that first touch, in m/s.
    pub touchdown_fall_m_s: Option<f32>,
    /// How long the episode ran, in seconds.
    pub duration_s: f32,
}

/// The numbers an episode gathers while it runs.
#[derive(Default)]
struct Tally {
    saves: u32,
    crash_cause: Option<CrashCause>,
    air_used_in_saves_kg: f32,
    peak_tilt_rad: f32,
    /// The motion just before the first touch.
    touchdown: Option<Touchdown>,
    /// When the current unbroken run of all four wheels down began.
    wheels_down_since_s: Option<f32>,
    /// The landing of the latest save (what the reward's save quality judges).
    last_save_landing: Option<Touchdown>,
}

/// A running episode: the world plus the numbers gathered so far.
pub struct Episode {
    world: World,
    rules: EpisodeConfig,
    seed: u64,
    limit_steps: u64,
    steps: u64,
    /// The car on the latest step, so the next step knows what it was just before.
    state: CarState,
    /// Anything (wheels, body or roof) touched the ground on the latest step.
    touching: bool,
    tally: Tally,
    /// Throttles on their way to the valves, one per control step of latency.
    delayed_throttles: VecDeque<[f32; NOZZLE_COUNT]>,
    /// What the controller asked for on the latest control step (the observation's
    /// `prev_action`, SPEC §9.2).
    commanded_throttles: [f32; NOZZLE_COUNT],
    /// What the valves are being told now (the commanded throttles, `latency` control
    /// steps late).
    applied_throttles: [f32; NOZZLE_COUNT],
    /// Sensor noise on what controllers see (all zero: none), and its generator.
    obs_noise: ObsNoise,
    noise_rng: ChaCha8Rng,
}

impl Episode {
    /// A fresh flat-ground world for `variation` (use `EpisodeVariation::none` for no
    /// randomization) with the car placed for `stage` from `seed`.
    pub fn start(
        variation: &EpisodeVariation,
        scenarios: &ScenariosConfig,
        stage: Stage,
        seed: u64,
    ) -> Result<Episode, PlaceCarError> {
        let mut world = World::new_on_flat_ground(
            &variation.vehicle,
            variation.mode,
            variation.ground_friction,
            seed,
        );
        start_episode(&mut world, stage, scenarios, seed)?;
        let rules = scenarios.episode;
        let delayed_throttles = vec![[0.0; NOZZLE_COUNT]; variation.latency_control_steps];
        Ok(Episode {
            state: world.car_state(),
            touching: false,
            world,
            rules,
            seed,
            limit_steps: (rules.time_limit_s * PHYSICS_HZ as f32).round() as u64,
            steps: 0,
            tally: Tally::default(),
            delayed_throttles: VecDeque::from(delayed_throttles),
            commanded_throttles: [0.0; NOZZLE_COUNT],
            applied_throttles: [0.0; NOZZLE_COUNT],
            obs_noise: variation.obs_noise,
            noise_rng: noise_rng(seed),
        })
    }

    /// What a controller sees now: the car's state with this episode's sensor noise
    /// (SPEC §9.8), and the observation built from that same state (SPEC §9.2), whose
    /// `prev_action` is what the controller last commanded.
    pub fn controller_view(&mut self) -> (CarState, Observation) {
        let mut state = self.state;
        add_state_noise(&mut state, &self.obs_noise, &mut self.noise_rng);
        let mut observation = observe(&self.world, &state);
        let prev_action = PREV_ACTION_INDEX..PREV_ACTION_INDEX + NOZZLE_COUNT;
        observation[prev_action].copy_from_slice(&self.commanded_throttles);
        (state, observation)
    }

    /// The observation a controller sees now (see `controller_view`).
    pub fn observe(&mut self) -> Observation {
        self.controller_view().1
    }

    /// The world, for controllers (`is_control_step`) and rendering.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The car on the latest step: what a controller sees.
    pub fn car_state(&self) -> CarState {
        self.state
    }

    /// Save events so far.
    pub fn saves(&self) -> u32 {
        self.tally.saves
    }

    /// How the car was moving just before the touchdown that led to the latest save;
    /// None before the first save.
    pub fn last_save_landing(&self) -> Option<Touchdown> {
        self.tally.last_save_landing
    }

    /// True if anything (a wheel, the body or the roof) touched the ground on the
    /// latest step; false while the car flies.
    pub fn anything_touching(&self) -> bool {
        self.touching
    }

    /// One physics step with the controller's latest throttles (0..1, nozzle order).
    /// On a control step they join the latency queue (SPEC §9.8), and the valves get the
    /// ones from `latency` control steps ago. Does nothing once the episode has ended.
    pub fn step(&mut self, throttles: [f32; NOZZLE_COUNT]) -> EpisodeStatus {
        if self.status() != EpisodeStatus::Running {
            return self.status();
        }
        if self.world.is_control_step() {
            self.commanded_throttles = throttles;
            self.delayed_throttles.push_back(throttles);
            if let Some(oldest) = self.delayed_throttles.pop_front() {
                self.applied_throttles = oldest;
            }
        }
        let before = self.state;
        self.world.set_controller_throttles(self.applied_throttles);
        self.world.step(InputFrame::default());
        self.steps += 1;
        self.state = self.world.car_state();
        self.tally.peak_tilt_rad = self.tally.peak_tilt_rad.max(self.world.tilt_rad());
        self.track_touchdown(&before);
        self.track_wheels_down();
        for event in self.world.drain_events() {
            self.record(event);
        }
        self.status()
    }

    /// SPEC §9.5: a crash ends the episode, and so does success.
    pub fn status(&self) -> EpisodeStatus {
        if let Some(cause) = self.tally.crash_cause {
            return EpisodeStatus::Crashed(cause);
        }
        if self.held_on_wheels_long_enough() {
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
            seed: self.seed,
            success: self.status() == EpisodeStatus::Succeeded,
            saves: self.tally.saves,
            crash_cause: self.tally.crash_cause,
            air_used_kg: self.world.air_used_kg(),
            air_used_in_saves_kg: self.tally.air_used_in_saves_kg,
            peak_tilt_deg: self.tally.peak_tilt_rad.to_degrees(),
            touchdown_spin_rad_s: self.tally.touchdown.map(|touchdown| touchdown.spin_rad_s),
            touchdown_fall_m_s: self.tally.touchdown.map(|touchdown| touchdown.fall_m_s),
            duration_s: self.world.time_s() as f32,
        }
    }

    /// The first time anything touches the ground, keeps the spin and fall speed from
    /// `before` this step (after it, the ground has already stopped the car).
    fn track_touchdown(&mut self, before: &CarState) {
        self.touching = self.state.wheels_down() > 0 || self.world.chassis_touching();
        if self.touching && self.tally.touchdown.is_none() {
            self.tally.touchdown = Some(Touchdown::of(before));
        }
    }

    /// Starts or breaks the run of all four wheels down that success needs.
    fn track_wheels_down(&mut self) {
        if self.state.wheels_down() < WHEEL_COUNT {
            self.tally.wheels_down_since_s = None;
        } else if self.tally.wheels_down_since_s.is_none() {
            self.tally.wheels_down_since_s = Some(self.world.time_s() as f32);
        }
    }

    /// Adds one event to the tally.
    fn record(&mut self, event: SimEvent) {
        match event {
            SimEvent::Save {
                air_used_kg,
                landing,
                ..
            } => {
                self.tally.saves += 1;
                self.tally.air_used_in_saves_kg += air_used_kg;
                self.tally.last_save_landing = Some(landing);
            }
            SimEvent::Crash { cause } => self.tally.crash_cause = Some(cause),
            _ => {}
        }
    }

    fn held_on_wheels_long_enough(&self) -> bool {
        match self.tally.wheels_down_since_s {
            Some(since_s) => self.world.time_s() as f32 - since_s >= self.rules.success_hold_s,
            None => false,
        }
    }
}
