//! Hybrid-gated training (DECISIONS 2026-10-21): PD flies the car, hidden from the policy,
//! until the game's own switch (`nl_control::HybridSwitch`) hands the nozzles to the AI.
//! Each AI stretch, from that handover to the gate's release, a crash or the time limit,
//! is one episode for the policy, so it trains on exactly the flights it gets in the game.

use nl_control::HybridSwitch;
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::obs::Observation;
use nl_sim::reward::RewardTerms;
use nl_sim::training::{FinishedEpisode, TrainingEnv};
use rand::Rng;
use rand_chacha::ChaCha8Rng;

use crate::env_slot::{NOT_RESET, SlotStep, controller_ctx, slot_step, start_env};

/// Hybrid gating gives up after this many episodes in a row that PD flew alone: the
/// stage mix almost never upsets the car, so training would only spin.
const MAX_EPISODES_WITHOUT_AI: u32 = 10_000;

/// What a hidden PD stretch ended with.
struct PdStretch {
    /// The observation at the gate's next handover: the policy's next first observation.
    observation: Observation,
    /// The row of the sim episode the AI flew in, if it ended under PD.
    finished: Option<FinishedEpisode>,
    /// The reward of that episode's last step, if it ended in success.
    success_terms: Option<RewardTerms>,
}

/// PD and the gate for one environment, and whether the AI has flown this sim episode.
#[derive(Debug, Clone)]
pub struct HybridPd {
    switch: HybridSwitch,
    /// The current sim episode has had at least one AI stretch.
    ai_flew: bool,
}

impl HybridPd {
    /// Training through `switch` (PD and the gate, built as the game builds them).
    pub fn new(switch: HybridSwitch) -> HybridPd {
        HybridPd {
            switch,
            ai_flew: false,
        }
    }

    /// Starts the sim episode with `seed` and returns the observation where the gate
    /// first hands the AI the car. PD flies until then, starting the next episodes from
    /// `seeds` if it flies this one alone.
    pub fn start(
        &mut self,
        env: &mut TrainingEnv,
        seeds: &mut ChaCha8Rng,
        seed: u64,
    ) -> Result<Observation, String> {
        self.begin(env, seed)?;
        if self.decide(env) {
            return Ok(env.controller_view().1);
        }
        // A fresh episode has no AI stretch yet, so no row can come back here.
        Ok(self.fly_pd(env, seeds)?.observation)
    }

    /// One AI control step with `throttle_actions`. The policy's episode ends with the
    /// sim episode, or when the gate hands back to PD, which then flies hidden until the
    /// next handover. The gate releases a settled car before the sim judges success
    /// (`save_hold_s` < `success_hold_s`), so if that sim episode then succeeds under PD
    /// before the AI flies again, the release step earns its success and air-left
    /// bonuses. A later crash or time limit under PD is not the landing's doing.
    pub fn step(
        &mut self,
        env: &mut TrainingEnv,
        seeds: &mut ChaCha8Rng,
        throttle_actions: [f32; NOZZLE_COUNT],
    ) -> Result<SlotStep, String> {
        let outcome = env.step(throttle_actions).ok_or(NOT_RESET)?;
        let mut step = slot_step(&outcome);
        if let Some(episode) = outcome.finished {
            let seed = seeds.next_u64();
            let next = self.start(env, seeds, seed)?;
            step.end_episode(outcome.observation, Some(episode), next);
            return Ok(step);
        }
        if self.decide(env) {
            return Ok(step);
        }
        // Released: the car is settled on its wheels, so the policy's episode is over.
        step.terminated = true;
        let stretch = self.fly_pd(env, seeds)?;
        if let Some(terms) = stretch.success_terms {
            step.terms.success += terms.success;
            step.terms.air_left += terms.air_left;
        }
        step.end_episode(outcome.observation, stretch.finished, stretch.observation);
        Ok(step)
    }

    /// Resets PD, the gate and `env` for the episode with `seed`.
    fn begin(&mut self, env: &mut TrainingEnv, seed: u64) -> Result<(), String> {
        self.switch.reset(seed);
        self.ai_flew = false;
        start_env(env, seed)?;
        Ok(())
    }

    /// One control step's choice on what the controllers see of `env` now: PD acts, then
    /// the gate updates (as in the game). True when the AI has the nozzles.
    fn decide(&mut self, env: &TrainingEnv) -> bool {
        let ai = self.switch.decide(&controller_ctx(env));
        self.ai_flew |= ai;
        ai
    }

    /// PD flies (the gate has just said PD) until the gate hands over, starting new
    /// episodes from `seeds` as old ones end.
    fn fly_pd(
        &mut self,
        env: &mut TrainingEnv,
        seeds: &mut ChaCha8Rng,
    ) -> Result<PdStretch, String> {
        let mut finished = None;
        let mut success_terms = None;
        let mut episodes_without_ai = 0;
        loop {
            let outcome = env
                .step_throttles(self.switch.pd_throttles())
                .ok_or(NOT_RESET)?;
            if let Some(episode) = outcome.finished {
                if self.ai_flew {
                    if episode.result.success {
                        success_terms = Some(outcome.terms);
                    }
                    finished = Some(episode);
                } else {
                    episodes_without_ai += 1;
                    check_ai_gets_flights(episodes_without_ai)?;
                }
                self.begin(env, seeds.next_u64())?;
            }
            if self.decide(env) {
                let observation = env.controller_view().1;
                return Ok(PdStretch {
                    observation,
                    finished,
                    success_terms,
                });
            }
        }
    }
}

/// An error once PD has flown too many episodes in a row without a handover.
fn check_ai_gets_flights(episodes_without_ai: u32) -> Result<(), String> {
    if episodes_without_ai < MAX_EPISODES_WITHOUT_AI {
        return Ok(());
    }
    Err(format!(
        "BatchedSim: Hybrid gating ran {episodes_without_ai} episodes in a row without \
         the gate handing the car to the AI; add a stage that upsets the car (A, E or F) \
         at a level that tips it past the gate's angle"
    ))
}
