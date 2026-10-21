//! One environment inside a `BatchedSim`: a `TrainingEnv`, the generator that draws its
//! episode seeds, and SPEC §9.1 auto-reset (an episode that ends hands back its last
//! observation and starts the next episode straight away). With residual control the
//! slot also runs PD and adds the action to PD's throttles. Plain Rust, no Python types.

use nl_control::{ControlCtx, RESIDUAL_ACTION_COUNT, ResidualPd};
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::input::InputFrame;
use nl_sim::obs::Observation;
use nl_sim::reward::RewardTerms;
use nl_sim::scenarios::Stage;
use nl_sim::training::{FinishedEpisode, StageChoice, StepOutcome, TrainingConfig, TrainingEnv};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Mixed into the batch seed before it keys the slots' seed streams, so those streams
/// never equal the generators an episode builds from its own seed (scenarios use
/// ChaCha8 stream 1 of the episode seed, Chaos stream 0).
const SLOT_SEED_KEY: u64 = 0x6e6c_5f73_6c6f_7473; // "nl_slots" in ASCII

/// What one environment produced on one step.
pub struct SlotStep {
    /// The next observation: the new episode's first one if the episode just ended.
    pub observation: Observation,
    /// The reward, term by term.
    pub terms: RewardTerms,
    /// The episode ended by its own rules (success or crash).
    pub terminated: bool,
    /// The episode hit its time limit.
    pub truncated: bool,
    /// When the episode ended: its row and its last observation (SB3's
    /// `terminal_observation`).
    pub finished: Option<(FinishedEpisode, Observation)>,
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
}

impl EnvSlot {
    /// Slot number `index` of a batch started from `batch_seed`; `residual` turns on
    /// residual control around that PD.
    pub fn new(
        config: TrainingConfig,
        stage: Stage,
        batch_seed: u64,
        index: u64,
        residual: Option<ResidualPd>,
    ) -> EnvSlot {
        EnvSlot {
            env: TrainingEnv::new(config, stage),
            seeds: slot_seed_stream(batch_seed, index),
            residual,
        }
    }

    /// Restarts this slot's stream of episode seeds as slot `index` of `batch_seed`.
    pub fn reseed(&mut self, batch_seed: u64, index: u64) {
        self.seeds = slot_seed_stream(batch_seed, index);
    }

    /// Starts a new episode from `seed`, or from the next seed of this slot's stream.
    pub fn reset(&mut self, seed: Option<u64>) -> Result<Observation, String> {
        let seed = seed.unwrap_or_else(|| self.seeds.next_u64());
        if let Some(residual) = &mut self.residual {
            residual.reset(seed);
        }
        self.env
            .reset(seed)
            .map_err(|error| format!("episode seed {seed}: {error}"))
    }

    /// One control step; an episode that ends here is replaced by a fresh one. The last
    /// action is the residual gate, unused without gated residual control.
    pub fn step(&mut self, action: [f32; RESIDUAL_ACTION_COUNT]) -> Result<SlotStep, String> {
        let outcome = self
            .step_env(action)
            .ok_or("BatchedSim: call reset() before step()")?;
        let mut observation = outcome.observation;
        let mut finished = None;
        if let Some(episode) = outcome.finished {
            finished = Some((episode, observation));
            observation = self.reset(None)?;
        }
        Ok(SlotStep {
            observation,
            terms: outcome.terms,
            terminated: outcome.terminated,
            truncated: outcome.truncated,
            finished,
        })
    }

    /// Steps the environment with `action`: as the throttles (SPEC §9.3), or with
    /// residual control as a nudge to what PD makes of the same noisy view the policy
    /// acted on.
    fn step_env(&mut self, action: [f32; RESIDUAL_ACTION_COUNT]) -> Option<StepOutcome> {
        let Some(residual) = &mut self.residual else {
            let mut throttle_actions = [0.0; NOZZLE_COUNT];
            throttle_actions.copy_from_slice(&action[..NOZZLE_COUNT]);
            return self.env.step(throttle_actions);
        };
        let (state, observation) = self.env.controller_view();
        let ctx = ControlCtx {
            input: InputFrame::default(),
            state,
            observation,
        };
        let throttles = residual.act(&ctx, action);
        self.env.step_throttles(throttles)
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

/// The generator of slot `index`'s episode seeds: its own ChaCha8 stream of the keyed
/// batch seed.
fn slot_seed_stream(batch_seed: u64, index: u64) -> ChaCha8Rng {
    let mut seeds = ChaCha8Rng::seed_from_u64(batch_seed ^ SLOT_SEED_KEY);
    seeds.set_stream(index);
    seeds
}
