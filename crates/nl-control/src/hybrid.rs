//! The Hybrid controller (DECISIONS 2026-10-21): the AI saves upsets, PD does the rest. A
//! `HybridGate` hands the nozzles to the AI when the car tilts past SPEC §7.9's upset
//! angle and back to PD once the car has settled the way a save needs. The game, both eval runners and
//! training (`nl-py`) share the gate, so the policy is trained and judged under one rule.

use nl_sim::car_state::CarState;
use nl_sim::conventions::{CONTROL_DT_S, NOZZLE_COUNT, WHEEL_COUNT};
use nl_sim::vehicle_config::VehicleConfig;

use crate::{Ai, ControlCtx, Controller, Pd};

/// When the gate switches, all read from vehicle.toml `[events]` (SPEC §7.9), so a
/// trained policy is never judged under a gate tuned after it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HybridGateSettings {
    /// The AI takes over above this tilt, in degrees (`upset_tilt_deg`).
    pub enter_tilt_deg: f32,
    /// PD takes back over only below this tilt, in degrees (`save_tilt_deg`)...
    pub release_tilt_deg: f32,
    /// ...turning slower than this, in rad/s (`save_max_spin_rad_s`)...
    pub release_spin_rad_s: f32,
    /// ...on all four wheels, held this long, in seconds (`save_hold_s`).
    pub release_hold_s: f32,
}

impl HybridGateSettings {
    /// The settings from the car's upset and save thresholds.
    pub fn for_vehicle(vehicle: &VehicleConfig) -> HybridGateSettings {
        let events = &vehicle.events;
        HybridGateSettings {
            enter_tilt_deg: events.upset_tilt_deg,
            release_tilt_deg: events.save_tilt_deg,
            release_spin_rad_s: events.save_max_spin_rad_s,
            release_hold_s: events.save_hold_s,
        }
    }
}

/// Decides each control step whether the AI or PD has the nozzles. A latch: it switches
/// to the AI at once, but back to PD only after a settled hold, so a bounce on landing
/// never flicks control back and forth.
#[derive(Debug, Clone, PartialEq)]
pub struct HybridGate {
    settings: HybridGateSettings,
    ai_in_control: bool,
    /// How long the car has been settled on its wheels while the AI is in control, in s.
    settled_s: f32,
}

impl HybridGate {
    /// A gate with PD in control.
    pub fn new(settings: HybridGateSettings) -> HybridGate {
        HybridGate {
            settings,
            ai_in_control: false,
            settled_s: 0.0,
        }
    }

    /// Back to PD in control, at the start of an episode or on a respawn.
    pub fn reset(&mut self) {
        self.ai_in_control = false;
        self.settled_s = 0.0;
    }

    /// Updates the gate for one control step (`CONTROL_DT_S`) and says whether the AI
    /// has the nozzles. Call it once per control step with the state the controllers see.
    pub fn update(&mut self, state: &CarState) -> bool {
        if !self.ai_in_control {
            self.ai_in_control = self.is_upset(state);
            self.settled_s = 0.0;
            return self.ai_in_control;
        }
        self.settled_s = if self.is_settled(state) {
            self.settled_s + CONTROL_DT_S
        } else {
            0.0
        };
        if self.settled_s >= self.settings.release_hold_s {
            self.reset();
        }
        self.ai_in_control
    }

    /// True while the AI has the nozzles (as of the last `update`).
    pub fn ai_in_control(&self) -> bool {
        self.ai_in_control
    }

    /// Tilted past SPEC §7.9's upset angle. Being airborne alone is not enough: PD lands
    /// upright jumps and hops almost perfectly, and handing those to the AI as well cut
    /// Stage D success from 99% to 12% (EXPERIMENTS 2026-10-21).
    fn is_upset(&self, state: &CarState) -> bool {
        // Comparing cosines avoids acos; libm keeps it identical on every platform.
        state.up_alignment() < libm::cosf(self.settings.enter_tilt_deg.to_radians())
    }

    /// SPEC §7.9's save condition, without the hold: four wheels down, nearly level,
    /// barely turning.
    fn is_settled(&self, state: &CarState) -> bool {
        let level = state.up_alignment() > libm::cosf(self.settings.release_tilt_deg.to_radians());
        state.wheels_down() == WHEEL_COUNT
            && level
            && state.spin_rad_s() < self.settings.release_spin_rad_s
    }
}

/// The AI flies, PD drives: each control step the gate picks whose throttles go out.
/// Both controllers run every step, so PD's warm start stays current for a hand-back.
#[derive(Debug, Clone, PartialEq)]
pub struct Hybrid {
    gate: HybridGate,
    pd: Pd,
    ai: Ai,
}

impl Hybrid {
    /// PD and the AI (both built for the car) behind a gate with `settings`.
    pub fn new(pd: Pd, ai: Ai, settings: HybridGateSettings) -> Hybrid {
        Hybrid {
            gate: HybridGate::new(settings),
            pd,
            ai,
        }
    }

    /// True while the AI has the nozzles, for the HUD.
    pub fn ai_in_control(&self) -> bool {
        self.gate.ai_in_control()
    }
}

impl Controller for Hybrid {
    fn reset(&mut self, seed: u64) {
        self.gate.reset();
        self.pd.reset(seed);
        self.ai.reset(seed);
    }

    fn act(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT] {
        let pd_throttles = self.pd.act(ctx);
        if self.gate.update(&ctx.state) {
            self.ai.act(ctx)
        } else {
            pd_throttles
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The repo's thresholds: enter above 60°, release below 15° and 1.5 rad/s, held 0.5 s.
    fn settings() -> HybridGateSettings {
        HybridGateSettings {
            enter_tilt_deg: 60.0,
            release_tilt_deg: 15.0,
            release_spin_rad_s: 1.5,
            release_hold_s: 0.5,
        }
    }

    /// A car tilted `tilt_deg` about its roll axis, with `wheels` of its wheels down.
    fn car(tilt_deg: f32, wheels: usize) -> CarState {
        let tilt_rad = tilt_deg.to_radians();
        let mut wheel_contact = [false; WHEEL_COUNT];
        for down in wheel_contact.iter_mut().take(wheels) {
            *down = true;
        }
        CarState {
            gravity_dir_body: [0.0, -tilt_rad.cos(), tilt_rad.sin()],
            wheel_contact,
            ..CarState::default()
        }
    }

    /// Control steps in `seconds`, rounded up.
    fn steps(seconds: f32) -> usize {
        (seconds / CONTROL_DT_S).ceil() as usize
    }

    #[test]
    fn a_car_driving_on_its_wheels_stays_with_pd() {
        let mut gate = HybridGate::new(settings());

        let ai = gate.update(&car(5.0, 4));

        assert!(!ai);
    }

    #[test]
    fn one_wheel_lifting_on_a_bump_stays_with_pd() {
        let mut gate = HybridGate::new(settings());

        let ai = gate.update(&car(5.0, 3));

        assert!(!ai);
    }

    #[test]
    fn an_upright_jump_stays_with_pd() {
        let mut gate = HybridGate::new(settings());

        let ai = gate.update(&car(20.0, 0));

        assert!(!ai);
    }

    #[test]
    fn a_car_tumbling_past_the_upset_angle_in_the_air_hands_over_at_once() {
        let mut gate = HybridGate::new(settings());

        let ai = gate.update(&car(90.0, 0));

        assert!(ai);
    }

    #[test]
    fn tipping_past_the_upset_angle_on_four_wheels_hands_over_to_the_ai() {
        let mut gate = HybridGate::new(settings());

        let ai_at_55 = gate.update(&car(55.0, 4));
        let ai_at_65 = gate.update(&car(65.0, 4));

        assert!(!ai_at_55);
        assert!(ai_at_65);
    }

    #[test]
    fn the_ai_keeps_control_until_the_car_has_settled_for_the_hold() {
        let mut gate = HybridGate::new(settings());
        gate.update(&car(90.0, 0));

        for _ in 0..steps(0.5) - 1 {
            assert!(gate.update(&car(2.0, 4)));
        }
        let ai = gate.update(&car(2.0, 4));

        assert!(!ai);
    }

    #[test]
    fn a_bounce_during_the_hold_restarts_it() {
        let mut gate = HybridGate::new(settings());
        gate.update(&car(90.0, 0));
        for _ in 0..steps(0.4) {
            gate.update(&car(2.0, 4));
        }

        gate.update(&car(2.0, 2));
        for _ in 0..steps(0.4) {
            assert!(gate.update(&car(2.0, 4)));
        }
    }

    #[test]
    fn a_spinning_or_tilted_landing_does_not_count_as_settled() {
        let mut gate = HybridGate::new(settings());
        gate.update(&car(90.0, 0));
        let mut spinning = car(2.0, 4);
        spinning.angular_velocity_body_rad_s = [0.0, 2.0, 0.0];

        for _ in 0..steps(1.0) {
            assert!(gate.update(&spinning));
            assert!(gate.update(&car(20.0, 4)));
        }
    }

    #[test]
    fn reset_gives_the_nozzles_back_to_pd() {
        let mut gate = HybridGate::new(settings());
        gate.update(&car(90.0, 0));

        gate.reset();

        assert!(!gate.ai_in_control());
    }
}
