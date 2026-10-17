//! One reinforcement-learning environment (SPEC §9.1): the policy's action in, the
//! observation, reward and end flags out, one control step (2 physics steps) at a time.
//! It steps `episode::Episode`, so success, crash and time limit mean what they mean
//! in eval. `nl-py`'s `BatchedSim` runs many of these side by side; nothing about the
//! reward or the episode rules lives in Python (CLAUDE.md golden rule 1).

use serde::Serialize;

use crate::car_state::Touchdown;
use crate::conventions::{NOZZLE_COUNT, PHYSICS_DT_S};
use crate::episode::{Episode, EpisodeResult, EpisodeStatus};
use crate::modes::ModeParams;
use crate::obs::Observation;
use crate::randomization::EpisodeVariation;
use crate::reward::{RewardConfig, RewardTerms, StepSummary, reward_terms};
use crate::scenarios::{ScenariosConfig, Stage};
use crate::vehicle_config::VehicleConfig;
use crate::world::PlaceCarError;

/// Everything a training environment is built from.
#[derive(Debug, Clone)]
pub struct TrainingConfig {
    /// The car (vehicle.toml).
    pub vehicle: VehicleConfig,
    /// The physics preset episodes run in.
    pub mode: ModeParams,
    /// Starts and episode rules (scenarios.toml).
    pub scenarios: ScenariosConfig,
    /// Reward weights (reward.toml).
    pub reward: RewardConfig,
    /// Draw each episode's car and world from `scenarios.randomization` (SPEC §9.8).
    /// Training turns this on; eval leaves it off.
    pub randomize: bool,
}

/// What one control step produced.
#[derive(Debug, Clone, PartialEq)]
pub struct StepOutcome {
    /// What the policy sees next.
    pub observation: Observation,
    /// The reward for this step, term by term (`terms.total()` is what PPO gets).
    pub terms: RewardTerms,
    /// The episode ended by its own rules: success or crash (SPEC §9.5).
    pub terminated: bool,
    /// The episode was cut off by the time limit.
    pub truncated: bool,
    /// The finished episode's row, when it ended on this step.
    pub finished: Option<FinishedEpisode>,
}

/// A finished episode: the eval row plus what training logs per episode. As JSON it is
/// one flat object: the `EpisodeResult` fields plus `stage`, `level` and `control_steps`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FinishedEpisode {
    /// The same row the eval runner writes.
    #[serde(flatten)]
    pub result: EpisodeResult,
    /// Which stage it was.
    pub stage: Stage,
    /// Its curriculum level.
    pub level: u32,
    /// How many control steps it lasted.
    pub control_steps: u32,
}

/// What happened to the episode during one control step, for the reward.
#[derive(Debug, Clone, Copy)]
struct StepEvents {
    saved: bool,
    crashed: bool,
    succeeded: bool,
    hard_landings: u32,
}

/// A running training environment. Call `reset` before the first `step`.
pub struct TrainingEnv {
    config: TrainingConfig,
    /// The stage and level the next `reset` starts.
    next_stage: Stage,
    next_level: u32,
    /// The stage and level the running episode was started with.
    stage: Stage,
    level: u32,
    episode: Option<Episode>,
    /// The throttles of the last control step (for the smoothness cost).
    previous_throttles: [f32; NOZZLE_COUNT],
    /// How long nothing (wheels, body or roof) has touched anything, in s.
    flying_s: f32,
    control_steps: u32,
}

impl TrainingEnv {
    /// An environment that will run `stage` at curriculum level 0.
    pub fn new(config: TrainingConfig, stage: Stage) -> TrainingEnv {
        TrainingEnv {
            config,
            next_stage: stage,
            next_level: 0,
            stage,
            level: 0,
            episode: None,
            previous_throttles: [0.0; NOZZLE_COUNT],
            flying_s: 0.0,
            control_steps: 0,
        }
    }

    /// Chooses the stage and curriculum level for the episodes that start after this
    /// (the running episode keeps the ones it started with).
    pub fn set_stage_and_level(&mut self, stage: Stage, level: u32) {
        self.next_stage = stage;
        self.next_level = level;
    }

    /// Starts a new episode from `seed` and returns its first observation. On an
    /// error there is no running episode until the next successful `reset`.
    pub fn reset(&mut self, seed: u64) -> Result<Observation, PlaceCarError> {
        self.episode = None;
        let (stage, level) = (self.next_stage, self.next_level);
        let mut episode = self.start_episode(stage, level, seed)?;
        let observation = episode.observe();
        self.episode = Some(episode);
        self.stage = stage;
        self.level = level;
        self.previous_throttles = [0.0; NOZZLE_COUNT];
        self.flying_s = 0.0;
        self.control_steps = 0;
        Ok(observation)
    }

    /// One control step with the policy's `action` (each in −1..1, SPEC §9.3). Returns
    /// None before the first `reset` or after the episode ended (call `reset` again).
    pub fn step(&mut self, action: [f32; NOZZLE_COUNT]) -> Option<StepOutcome> {
        let throttles = throttles_from_action(action);
        let mut episode = self.episode.take()?;
        if episode.status() != EpisodeStatus::Running {
            self.episode = Some(episode);
            return None;
        }
        let saves_before = episode.saves();
        let hard_landings_before = episode.hard_landings();
        let air_before_kg = episode.world().air_used_kg();
        // Physics steps until the next control step (SPEC §7.1: two of them), or the end.
        loop {
            episode.step(throttles);
            self.track_flight(&episode);
            let ended = episode.status() != EpisodeStatus::Running;
            if ended || episode.world().is_control_step() {
                break;
            }
        }
        self.control_steps += 1;
        let status = episode.status();
        let crashed = matches!(status, EpisodeStatus::Crashed(_));
        let step_events = StepEvents {
            saved: episode.saves() > saves_before,
            crashed,
            succeeded: status == EpisodeStatus::Succeeded,
            hard_landings: episode.hard_landings() - hard_landings_before,
        };
        let air_used_kg = episode.world().air_used_kg() - air_before_kg;
        let summary = self.summarise(&episode, throttles, air_used_kg, step_events);
        self.previous_throttles = throttles;
        let outcome = StepOutcome {
            observation: episode.observe(),
            terms: reward_terms(&summary, &self.config.reward),
            terminated: crashed || status == EpisodeStatus::Succeeded,
            truncated: status == EpisodeStatus::TimedOut,
            finished: self.finished(&episode, status),
        };
        self.episode = Some(episode);
        Some(outcome)
    }

    /// A new episode of `stage` at `level` from `seed`, with a randomized car and world
    /// (SPEC §9.8) when `randomize` is on.
    fn start_episode(&self, stage: Stage, level: u32, seed: u64) -> Result<Episode, PlaceCarError> {
        let config = &self.config;
        let scenarios = config.scenarios.at_level(stage, level);
        let randomization = config.randomize.then_some(&scenarios.randomization);
        let variation =
            EpisodeVariation::for_seed(randomization, &config.vehicle, config.mode, seed);
        Episode::start(&variation, &scenarios, stage, seed)
    }

    /// Counts flight time: how long nothing (wheels, body or roof) has touched anything.
    fn track_flight(&mut self, episode: &Episode) {
        if episode.anything_touching() {
            self.flying_s = 0.0;
        } else {
            self.flying_s += PHYSICS_DT_S;
        }
    }

    /// The numbers the reward needs for this control step.
    fn summarise(
        &self,
        episode: &Episode,
        throttles: [f32; NOZZLE_COUNT],
        air_used_kg: f32,
        events: StepEvents,
    ) -> StepSummary {
        let state = episode.car_state();
        let mode = episode.world().mode();
        // Only read when `saved`, and then the save just set them (SPEC §9.4).
        let landing = episode.last_save_landing().unwrap_or(Touchdown::of(&state));
        let quality = episode.last_save_quality().unwrap_or_default();
        StepSummary {
            up_alignment: state.up_alignment(),
            spin_rad_s: state.spin_rad_s(),
            landing_spin_rad_s: landing.spin_rad_s,
            landing_fall_m_s: landing.fall_m_s,
            landing_heading_error_rad: quality.heading_error_rad,
            landing_speed_kept: quality.speed_kept,
            air_used_kg,
            tank_capacity_kg: mode.tank_capacity_kg,
            unlimited_air: mode.unlimited_air,
            throttles,
            previous_throttles: self.previous_throttles,
            airborne_s: self.flying_s,
            saved: events.saved,
            crashed: events.crashed,
            hard_landings: events.hard_landings,
            unsettled: episode.unsettled_after_touchdown(),
            succeeded: events.succeeded,
            air_left_fraction: episode.world().air_fraction(),
        }
    }

    /// The finished-episode row, if the episode just ended.
    fn finished(&self, episode: &Episode, status: EpisodeStatus) -> Option<FinishedEpisode> {
        if status == EpisodeStatus::Running {
            return None;
        }
        Some(FinishedEpisode {
            result: episode.result(),
            stage: self.stage,
            level: self.level,
            control_steps: self.control_steps,
        })
    }
}

/// SPEC §9.3: an action in −1..1 becomes a throttle `(a + 1) / 2` in 0..1. Values
/// outside the range are clamped, and NaN or infinity closes the valve.
pub fn throttles_from_action(action: [f32; NOZZLE_COUNT]) -> [f32; NOZZLE_COUNT] {
    let mut throttles = [0.0; NOZZLE_COUNT];
    for (index, value) in action.iter().enumerate() {
        if value.is_finite() {
            throttles[index] = ((value + 1.0) / 2.0).clamp(0.0, 1.0);
        }
    }
    throttles
}
