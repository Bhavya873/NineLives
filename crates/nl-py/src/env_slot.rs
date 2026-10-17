//! One environment inside a `BatchedSim`: a `TrainingEnv`, the generator that draws its
//! episode seeds, and SPEC §9.1 auto-reset (an episode that ends hands back its last
//! observation and starts the next episode straight away). Plain Rust, no Python types.

use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::obs::Observation;
use nl_sim::reward::RewardTerms;
use nl_sim::scenarios::Stage;
use nl_sim::training::{FinishedEpisode, TrainingConfig, TrainingEnv};
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
}

impl EnvSlot {
    /// Slot number `index` of a batch started from `batch_seed`.
    pub fn new(config: TrainingConfig, stage: Stage, batch_seed: u64, index: u64) -> EnvSlot {
        EnvSlot {
            env: TrainingEnv::new(config, stage),
            seeds: slot_seed_stream(batch_seed, index),
        }
    }

    /// Restarts this slot's stream of episode seeds as slot `index` of `batch_seed`.
    pub fn reseed(&mut self, batch_seed: u64, index: u64) {
        self.seeds = slot_seed_stream(batch_seed, index);
    }

    /// Starts a new episode from `seed`, or from the next seed of this slot's stream.
    pub fn reset(&mut self, seed: Option<u64>) -> Result<Observation, String> {
        let seed = seed.unwrap_or_else(|| self.seeds.next_u64());
        self.env
            .reset(seed)
            .map_err(|error| format!("episode seed {seed}: {error}"))
    }

    /// One control step; an episode that ends here is replaced by a fresh one.
    pub fn step(&mut self, action: [f32; NOZZLE_COUNT]) -> Result<SlotStep, String> {
        let outcome = self
            .env
            .step(action)
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

    /// The stage and curriculum level for the episodes this slot starts next.
    pub fn set_stage_and_level(&mut self, stage: Stage, level: u32) {
        self.env.set_stage_and_level(stage, level);
    }
}

/// The generator of slot `index`'s episode seeds: its own ChaCha8 stream of the keyed
/// batch seed.
fn slot_seed_stream(batch_seed: u64, index: u64) -> ChaCha8Rng {
    let mut seeds = ChaCha8Rng::seed_from_u64(batch_seed ^ SLOT_SEED_KEY);
    seeds.set_stream(index);
    seeds
}
