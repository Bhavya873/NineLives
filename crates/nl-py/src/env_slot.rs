//! One environment inside a `BatchedSim`: a `TrainingEnv`, the generator that draws its
//! episode seeds, and SPEC §9.1 auto-reset (an episode that ends hands back its last
//! observation and starts the next episode straight away). With residual control the
//! slot also runs PD and adds the action to PD's throttles; with Hybrid gating PD flies
//! hidden until the gate hands over, and each AI stretch is one episode for the policy.

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

/// Hybrid gating gives up after this many episodes in a row that PD flew alone: the
/// stage mix almost never upsets the car, so training would only spin.
const MAX_EPISODES_WITHOUT_AI: u32 = 10_000;

/// Stepping before the first `reset`.
const NOT_RESET: &str = "BatchedSim: call reset() before step()";

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

/// One environment and its seed generator.
pub struct EnvSlot {
    env: TrainingEnv,
    /// Draws the seed of each episode this slot starts by itself. Every slot uses its
    /// own ChaCha8 stream of the batch seed, so slots never repeat each other.
    seeds: ChaCha8Rng,
    /// Residual control: PD (reset with each episode) plus the scaled action. None: the
    /// action alone sets the throttles (SPEC §9.3).
    residual: Option<ResidualPd>,
    /// Hybrid gating: PD flies until the gate hands the car to the policy.
    hybrid: Option<HybridPd>,
}

impl EnvSlot {
    /// Slot number `index` of a batch started from `batch_seed`; `residual` turns on
    /// residual control around that PD, `hybrid` turns on Hybrid gating (not both).
    pub fn new(
        config: TrainingConfig,
        stage: Stage,
        batch_seed: u64,
        index: u64,
        residual: Option<ResidualPd>,
        hybrid: Option<HybridPd>,
    ) -> EnvSlot {
        EnvSlot {
            env: TrainingEnv::new(config, stage),
            seeds: slot_seed_stream(batch_seed, index),
            residual,
            hybrid,
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
        let observation = self.start_episode(seed)?;
        if self.hybrid.is_none() {
            return Ok(observation);
        }
        let (observation, _) = self.until_handover()?;
        Ok(observation)
    }

    /// One control step; an episode that ends here is replaced by a fresh one. The last
    /// action is the residual gate, unused without gated residual control.
    pub fn step(&mut self, action: [f32; RESIDUAL_ACTION_COUNT]) -> Result<SlotStep, String> {
        if self.hybrid.is_some() {
            return self.step_hybrid(action);
        }
        let outcome = self.step_env(action).ok_or(NOT_RESET)?;
        let mut step = slot_step(&outcome);
        if let Some(episode) = outcome.finished {
            step.terminal_observation = Some(outcome.observation);
            step.finished = Some(episode);
            step.observation = self.reset(None)?;
        }
        Ok(step)
    }

    /// Steps the environment with `action`: as the throttles (SPEC §9.3), or with
    /// residual control as a nudge to what PD makes of the same noisy view the policy
    /// acted on.
    fn step_env(&mut self, action: [f32; RESIDUAL_ACTION_COUNT]) -> Option<StepOutcome> {
        let Some(residual) = &mut self.residual else {
            return self.env.step(throttle_actions(action));
        };
        let ctx = controller_ctx(&self.env);
        let throttles = residual.act(&ctx, action);
        self.env.step_throttles(throttles)
    }

    /// One AI control step under Hybrid gating (the gate gave the AI this step when the
    /// last call returned). The policy's episode ends with the sim episode, or when the
    /// gate hands back to PD, which then flies hidden until the next handover.
    fn step_hybrid(&mut self, action: [f32; RESIDUAL_ACTION_COUNT]) -> Result<SlotStep, String> {
        let ctx = controller_ctx(&self.env);
        self.hybrid_pd()?.pd_throttles(&ctx);
        let outcome = self.env.step(throttle_actions(action)).ok_or(NOT_RESET)?;
        let mut step = slot_step(&outcome);
        if let Some(episode) = outcome.finished {
            step.terminal_observation = Some(outcome.observation);
            step.finished = Some(episode);
            step.observation = self.reset(None)?;
            return Ok(step);
        }
        let (state, _) = self.env.controller_view();
        if self.hybrid_pd()?.ai_in_control(&state) {
            return Ok(step);
        }
        // Released: the car is settled on its wheels, so the policy's episode is over.
        step.terminated = true;
        step.terminal_observation = Some(outcome.observation);
        let (observation, finished) = self.fly_pd()?;
        step.observation = observation;
        step.finished = finished;
        Ok(step)
    }

    /// Starts the next sim episode from `seed` (or the stream) with fresh controllers.
    fn start_episode(&mut self, seed: Option<u64>) -> Result<Observation, String> {
        let seed = seed.unwrap_or_else(|| self.seeds.next_u64());
        if let Some(residual) = &mut self.residual {
            residual.reset(seed);
        }
        if let Some(hybrid) = &mut self.hybrid {
            hybrid.reset(seed);
        }
        self.env
            .reset(seed)
            .map_err(|error| format!("episode seed {seed}: {error}"))
    }

    /// From a fresh episode: the observation where the gate first gives the AI the car,
    /// with PD flying (hidden) until then.
    fn until_handover(&mut self) -> Result<(Observation, Option<FinishedEpisode>), String> {
        let (state, observation) = self.env.controller_view();
        if self.hybrid_pd()?.ai_in_control(&state) {
            return Ok((observation, None));
        }
        self.fly_pd()
    }

    /// PD flies (the gate has just said PD) until the gate hands over, starting new
    /// episodes from the stream as old ones end. Returns the handover observation and
    /// the row of an episode the AI flew in that ended under PD, if any.
    fn fly_pd(&mut self) -> Result<(Observation, Option<FinishedEpisode>), String> {
        let mut finished = None;
        let mut episodes_without_ai = 0;
        loop {
            let ctx = controller_ctx(&self.env);
            let throttles = self.hybrid_pd()?.pd_throttles(&ctx);
            let outcome = self.env.step_throttles(throttles).ok_or(NOT_RESET)?;
            if let Some(episode) = outcome.finished {
                if self.hybrid_pd()?.ai_flew() {
                    finished = Some(episode);
                } else {
                    episodes_without_ai += 1;
                    check_ai_gets_flights(episodes_without_ai)?;
                }
                self.start_episode(None)?;
            }
            let (state, observation) = self.env.controller_view();
            if self.hybrid_pd()?.ai_in_control(&state) {
                return Ok((observation, finished));
            }
        }
    }

    /// The Hybrid PD; only called with gating on.
    fn hybrid_pd(&mut self) -> Result<&mut HybridPd, String> {
        self.hybrid
            .as_mut()
            .ok_or_else(|| "BatchedSim: Hybrid gating is off".to_string())
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

/// What a controller sees of `env` now: the noisy state and observation the policy
/// acted on, with no driver input (as in eval).
fn controller_ctx(env: &TrainingEnv) -> ControlCtx {
    let (state, observation) = env.controller_view();
    ControlCtx {
        input: InputFrame::default(),
        state,
        observation,
    }
}

/// A step's result before any episode bookkeeping.
fn slot_step(outcome: &StepOutcome) -> SlotStep {
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

/// An error once PD has flown too many episodes in a row without a handover.
fn check_ai_gets_flights(episodes_without_ai: u32) -> Result<(), String> {
    if episodes_without_ai < MAX_EPISODES_WITHOUT_AI {
        return Ok(());
    }
    Err(format!(
        "BatchedSim: Hybrid gating ran {episodes_without_ai} episodes in a row without \
         the gate handing the car to the AI; add a stage that upsets the car (A, B, E or F) \
         at a level that tips it past the gate's angle"
    ))
}

/// The generator of slot `index`'s episode seeds: its own ChaCha8 stream of the keyed
/// batch seed.
fn slot_seed_stream(batch_seed: u64, index: u64) -> ChaCha8Rng {
    let mut seeds = ChaCha8Rng::seed_from_u64(batch_seed ^ SLOT_SEED_KEY);
    seeds.set_stream(index);
    seeds
}
