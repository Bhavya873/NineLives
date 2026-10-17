//! The AI controller (SPEC §8): the trained policy (`nl_policy::Policy`) run on the
//! observation, its actions mapped to throttles the same way training maps them
//! (`(a + 1) / 2`, SPEC §9.3). It keeps no state between control steps.

use nl_policy::Policy;
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::training::throttles_from_action;

use crate::{ControlCtx, Controller};

/// Fires the nozzles the way the trained policy says (deterministic, mean action).
#[derive(Debug, Clone, PartialEq)]
pub struct Ai {
    policy: Policy,
}

impl Ai {
    /// The controller for a loaded policy.
    pub fn new(policy: Policy) -> Ai {
        Ai { policy }
    }
}

impl Controller for Ai {
    fn reset(&mut self, _seed: u64) {}

    fn act(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT] {
        throttles_from_action(self.policy.act(&ctx.observation))
    }
}
