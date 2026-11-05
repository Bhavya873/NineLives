//! One environment inside a `BatchedSim`: a `TrainingEnv`, the generator that draws its
//! episode seeds, and SPEC §9.1 auto-reset (an episode that ends hands back its last
//! observation and starts the next episode straight away). With residual control the
//! slot also runs PD and adds the action to PD's throttles; with Hybrid gating
//! (`hybrid_pd.rs`) PD flies hidden until the gate hands over.

use nl_control::{ControlCtx, RESIDUAL_ACTION_COUNT, ResidualPd};
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::input::InputFrame;
use nl_sim::obs::Observation;
use nl_sim::reward::RewardTerms;
use nl_sim::scenarios::Stage;
use nl_sim::training::{FinishedEpisode, StageChoice, StepOutcome, TrainingConfig, TrainingEnv};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::hybrid_pd::HybridPd;

/// Mixed into the batch seed before it keys the slots' seed streams, so those streams
/// never equal the generators an episode builds from its own seed (scenarios use
/// ChaCha8 stream 1 of the episode seed, Chaos stream 0).
const SLOT_SEED_KEY: u64 = 0x6e6c_5f73_6c6f_7473; // "nl_slots" in ASCII

/// Stepping before the first `reset`.
pub(crate) const NOT_RESET: &str = "BatchedSim: call reset() before step()";

/// How the policy's action reaches the nozzles.
#[derive(Debug, Clone)]
pub enum SlotControl {
    /// The action is the throttles (SPEC §9.3).
    Direct,
    /// PD (reset with each episode) plus the scaled action.
    Residual(ResidualPd),
    /// PD flies until the game's gate hands over; each AI stretch is an episode.
    Hybrid(HybridPd),
}

/// What one environment produced on one step.
pub struct SlotStep {
    /// The next observation: the policy's next episode's first one if its episode ended.
    pub observation: Observation,
    /// The reward, term by term.
    pub terms: RewardTerms,
    /// The policy's episode ended by its own rules (success, crash, or the Hybrid gate's
    /// release of a settled car).
    pub terminated: bool,
    /// The episode hit its time limit.
    pub truncated: bool,
    /// When the policy's episode ended: its last observation (SB3's
    /// `terminal_observation`).
    pub terminal_observation: Option<Observation>,
    /// A sim episode that ended since the last step: its row. With Hybrid gating it may
    /// have ended under PD after the policy's last stretch; episodes PD flew alone are
    /// not reported.
    pub finished: Option<FinishedEpisode>,
}

impl SlotStep {
    /// The policy's episode ended on this step: `last` was its final observation,
    /// `finished` the row of a sim episode that ended, `next` the next episode's start.
    pub fn end_episode(
        &mut self,
        last: Observation,
        finished: Option<FinishedEpisode>,
        next: Observation,
    ) {
        self.terminal_observation = Some(last);
        self.finished = finished;
        self.observation = next;
    }
}

/// One environment and its seed generator.
pub struct EnvSlot {
    env: TrainingEnv,
    /// Draws the seed of each episode this slot starts by itself. Every slot uses its
    /// own ChaCha8 stream of the batch seed, so slots never repeat each other.
    seeds: ChaCha8Rng,
    control: SlotControl,
}

impl EnvSlot {
    /// Slot number `index` of a batch started from `batch_seed`, its actions applied
    /// through `control`.
    pub fn new(
        config: TrainingConfig,
        stage: Stage,
        batch_seed: u64,
        index: u64,
        control: SlotControl,
    ) -> EnvSlot {
        EnvSlot {
            env: TrainingEnv::new(config, stage),
            seeds: slot_seed_stream(batch_seed, index),
            control,
        }
    }

    /// Restarts this slot's stream of episode seeds as slot `index` of `batch_seed`.
    pub fn reseed(&mut self, batch_seed: u64, index: u64) {
        self.seeds = slot_seed_stream(batch_seed, index);
    }

    /// Starts a new episode from `seed`, or from the next seed of this slot's stream.
    /// With Hybrid gating, returns the observation at the gate's first handover (later
    /// episodes come from the stream if PD flies this one alone).
    pub fn reset(&mut self, seed: Option<u64>) -> Result<Observation, String> {
        let seed = seed.unwrap_or_else(|| self.seeds.next_u64());
        match &mut self.control {
            SlotControl::Direct => {}
            SlotControl::Residual(residual) => residual.reset(seed),
            SlotControl::Hybrid(hybrid) => {
                return hybrid.start(&mut self.env, &mut self.seeds, seed);
            }
        }
        start_env(&mut self.env, seed)
    }

    /// One control step; an episode that ends here is replaced by a fresh one. The last
    /// action is the residual gate, unused without gated residual control.
    pub fn step(&mut self, action: [f32; RESIDUAL_ACTION_COUNT]) -> Result<SlotStep, String> {
        let outcome = match &mut self.control {
            SlotControl::Direct => self.env.step(throttle_actions(action)),
            SlotControl::Residual(residual) => {
                // A nudge to what PD makes of the same noisy view the policy acted on.
                let ctx = controller_ctx(&self.env);
                self.env.step_throttles(residual.act(&ctx, action))
            }
            SlotControl::Hybrid(hybrid) => {
                return hybrid.step(&mut self.env, &mut self.seeds, throttle_actions(action));
            }
        };
        let outcome = outcome.ok_or(NOT_RESET)?;
        let mut step = slot_step(&outcome);
        if let Some(episode) = outcome.finished {
            let next = self.reset(None)?;
            step.end_episode(outcome.observation, Some(episode), next);
        }
        Ok(step)
    }

    /// The stage and curriculum level for the episodes this slot starts next.
    pub fn set_stage_and_level(&mut self, stage: Stage, level: u32) {
        self.env.set_stage_and_level(stage, level);
    }

    /// The stages (weights and levels) this slot's next episodes pick from.
    pub fn set_stage_mix(&mut self, mix: Vec<StageChoice>) -> Result<(), String> {
        self.env.set_stage_mix(mix)
    }
}

/// Starts `env`'s episode from `seed`; its first observation.
pub(crate) fn start_env(env: &mut TrainingEnv, seed: u64) -> Result<Observation, String> {
    env.reset(seed)
        .map_err(|error| format!("episode seed {seed}: {error}"))
}

/// What a controller sees of `env` now: the noisy state and observation the policy
/// acted on and the thrusters' state, with no driver input (as in eval).
pub(crate) fn controller_ctx(env: &TrainingEnv) -> ControlCtx {
    let (state, observation) = env.controller_view();
    ControlCtx {
        input: InputFrame::default(),
        state,
        observation,
        // As eval's `ControlCtx::for_episode`, so a base PD plans with the same
        // allocation the game and eval use.
        actuators: env.actuator_state(),
        // Training stays on the old PD (no look-ahead) until the policy is retrained.
        landing: None,
    }
}

/// A step's result before any episode bookkeeping.
pub(crate) fn slot_step(outcome: &StepOutcome) -> SlotStep {
    SlotStep {
        observation: outcome.observation,
        terms: outcome.terms,
        terminated: outcome.terminated,
        truncated: outcome.truncated,
        terminal_observation: None,
        finished: None,
    }
}

/// The first 10 actions: one per nozzle (the 11th is the residual gate).
fn throttle_actions(action: [f32; RESIDUAL_ACTION_COUNT]) -> [f32; NOZZLE_COUNT] {
    let mut throttle_actions = [0.0; NOZZLE_COUNT];
    throttle_actions.copy_from_slice(&action[..NOZZLE_COUNT]);
    throttle_actions
}

/// The generator of slot `index`'s episode seeds: its own ChaCha8 stream of the keyed
/// batch seed.
fn slot_seed_stream(batch_seed: u64, index: u64) -> ChaCha8Rng {
    let mut seeds = ChaCha8Rng::seed_from_u64(batch_seed ^ SLOT_SEED_KEY);
    seeds.set_stream(index);
    seeds
}
