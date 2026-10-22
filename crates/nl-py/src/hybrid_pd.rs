//! Hybrid-gated training (DECISIONS 2026-10-21): PD flies the car, hidden from the policy,
//! until the game's own `HybridGate` hands the nozzles to the AI. Each AI stretch, from
//! that handover to the gate's release, a crash or the time limit, is one episode for the
//! policy, so it trains on exactly the flights it gets in the game. `EnvSlot` runs the loop.

use nl_control::{ControlCtx, Controller, HybridGate, HybridGateSettings, Pd};
use nl_sim::car_state::CarState;
use nl_sim::conventions::NOZZLE_COUNT;

/// PD and the gate for one environment.
#[derive(Debug, Clone)]
pub struct HybridPd {
    pd: Pd,
    gate: HybridGate,
    /// The current sim episode has had at least one AI stretch.
    ai_flew: bool,
}

impl HybridPd {
    /// `pd` behind a gate with `settings` (the game's, from vehicle.toml).
    pub fn new(pd: Pd, settings: HybridGateSettings) -> HybridPd {
        HybridPd {
            pd,
            gate: HybridGate::new(settings),
            ai_flew: false,
        }
    }

    /// Back to PD in control at the start of the episode with `seed`.
    pub fn reset(&mut self, seed: u64) {
        self.pd.reset(seed);
        self.gate.reset();
        self.ai_flew = false;
    }

    /// Updates the gate for one control step (call it once per step, with the state the
    /// controllers see); true when the AI has the nozzles.
    pub fn ai_in_control(&mut self, state: &CarState) -> bool {
        let ai = self.gate.update(state);
        self.ai_flew |= ai;
        ai
    }

    /// PD's throttles for this control step. Called on AI steps too, like the game's
    /// Hybrid, so PD's warm start is current when the gate hands back.
    pub fn pd_throttles(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT] {
        self.pd.act(ctx)
    }

    /// True once the AI has flown in the current sim episode.
    pub fn ai_flew(&self) -> bool {
        self.ai_flew
    }
}
