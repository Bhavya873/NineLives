//! Residual control (M7 approach 2): PD flies the car and a policy only nudges it. Each
//! control step the throttles are PD's plus a scaled policy action, clamped to 0..1, so a
//! zero action is exactly PD. The nudge can be scaled by the mode's thrust (as PD's own
//! allocation is) and opened or closed by a learned gate. Training (`nl-py`) uses it per
//! environment; the game would run the same code with the loaded policy.

use nl_sim::conventions::NOZZLE_COUNT;

use crate::{ControlCtx, Controller, Pd};

/// Actions a residual policy outputs: one nudge per nozzle, then the gate (read only
/// when `ResidualSettings::gated`).
pub const RESIDUAL_ACTION_COUNT: usize = NOZZLE_COUNT + 1;

/// How the policy's action becomes a throttle nudge (set in the training config).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResidualSettings {
    /// Throttle change per unit of action (finite, above 0).
    pub scale: f32,
    /// Divide the nudge by the mode's thrust scale, as PD's allocation does, so one unit
    /// of action asks for the same force in Realistic (×1) and Arcade (×2).
    pub force_scaled: bool,
    /// The policy's last action opens a gate g = (a + 1) / 2 in 0..1 that multiplies
    /// every nudge, so it can learn to leave PD alone where PD is already good.
    pub gated: bool,
}

/// PD plus a scaled residual from a policy.
#[derive(Debug, Clone, PartialEq)]
pub struct ResidualPd {
    pd: Pd,
    settings: ResidualSettings,
}

impl ResidualPd {
    /// PD (already built for the car) with residual actions applied per `settings`
    /// (the scale must be finite and above zero).
    pub fn new(pd: Pd, settings: ResidualSettings) -> Result<ResidualPd, String> {
        let scale = settings.scale;
        if !(scale.is_finite() && scale > 0.0) {
            return Err(format!(
                "residual scale must be a number above 0, not {scale}"
            ));
        }
        Ok(ResidualPd { pd, settings })
    }

    /// Forgets PD's warm start, at the start of an episode.
    pub fn reset(&mut self, seed: u64) {
        self.pd.reset(seed);
    }

    /// The throttles for this control step: PD's for `ctx` plus the scaled nudges (the
    /// first `NOZZLE_COUNT` actions; the last is the gate).
    pub fn act(
        &mut self,
        ctx: &ControlCtx,
        action: [f32; RESIDUAL_ACTION_COUNT],
    ) -> [f32; NOZZLE_COUNT] {
        let pd_throttles = self.pd.act(ctx);
        let mut nudges = [0.0; NOZZLE_COUNT];
        nudges.copy_from_slice(&action[..NOZZLE_COUNT]);
        let scale = self.nudge_scale(ctx.state.mode_thrust_scale, action[NOZZLE_COUNT]);
        add_residual(pd_throttles, nudges, scale)
    }

    /// Throttle per unit of nudge this step, after the thrust scaling and the gate.
    fn nudge_scale(&self, thrust_scale: f32, gate_action: f32) -> f32 {
        let mut scale = self.settings.scale;
        // Like `Allocation::solve`: a stronger mode needs less throttle for the same push.
        if self.settings.force_scaled && thrust_scale > 0.0 {
            scale /= thrust_scale;
        }
        if self.settings.gated {
            scale *= gate_opening(gate_action);
        }
        scale
    }
}

/// The gate's opening in 0..1 for its action in −1..1. A NaN or infinite action closes
/// it, so a broken policy output falls back to PD.
pub fn gate_opening(action: f32) -> f32 {
    if action.is_finite() {
        ((action + 1.0) * 0.5).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// `base + scale × action` per nozzle, clamped to 0..1. A NaN or infinite action adds
/// nothing, so a broken policy output falls back to the base controller.
pub fn add_residual(
    base: [f32; NOZZLE_COUNT],
    action: [f32; NOZZLE_COUNT],
    scale: f32,
) -> [f32; NOZZLE_COUNT] {
    let mut throttles = base;
    for index in 0..NOZZLE_COUNT {
        let nudge = if action[index].is_finite() {
            scale * action[index]
        } else {
            0.0
        };
        throttles[index] = (base[index] + nudge).clamp(0.0, 1.0);
    }
    throttles
}

#[cfg(test)]
mod tests {
    use nl_sim::car_state::CarState;
    use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
    use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};

    use super::*;
    use crate::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};

    /// PD for the repo's car in realistic mode.
    fn repo_pd() -> Pd {
        let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
        let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
            .unwrap()
            .realistic;
        let config = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();
        Pd::for_vehicle(&vehicle, mode, &config)
    }

    /// Scale 0.5 with force scaling and the gate on or off.
    fn settings(force_scaled: bool, gated: bool) -> ResidualSettings {
        ResidualSettings {
            scale: 0.5,
            force_scaled,
            gated,
        }
    }

    /// A car tumbling upside down in the air, so PD fires.
    fn tumbling_ctx() -> ControlCtx {
        let state = CarState {
            gravity_dir_body: [0.3, 0.9, 0.0],
            angular_velocity_body_rad_s: [2.0, 0.5, -1.0],
            height_m: 4.0,
            wheel_contact: [false; 4],
            ..CarState::default()
        };
        ControlCtx {
            state,
            ..ControlCtx::default()
        }
    }

    #[test]
    fn a_zero_action_gives_exactly_pds_throttles() {
        let mut pd = repo_pd();
        let mut residual = ResidualPd::new(repo_pd(), settings(true, true)).unwrap();
        let ctx = tumbling_ctx();

        let pd_throttles = pd.act(&ctx);
        let residual_throttles = residual.act(&ctx, [0.0; RESIDUAL_ACTION_COUNT]);

        assert!(pd_throttles.iter().any(|throttle| *throttle > 0.0));
        assert_eq!(residual_throttles, pd_throttles);
    }

    #[test]
    fn the_action_moves_each_throttle_by_scale_and_stays_in_range() {
        let base = [0.5, 0.0, 1.0, 0.2, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5];
        let action = [1.0, -1.0, 1.0, 0.4, -0.2, 0.0, f32::NAN, 0.0, 0.0, 0.0];

        let throttles = add_residual(base, action, 0.5);

        assert_eq!(throttles[0], 1.0);
        assert_eq!(throttles[1], 0.0); // already closed: cannot go below 0
        assert_eq!(throttles[2], 1.0); // already open: cannot go above 1
        assert!((throttles[3] - 0.4).abs() < 1e-6);
        assert!((throttles[4] - 0.4).abs() < 1e-6);
        assert_eq!(throttles[6], 0.5); // NaN adds nothing
    }

    #[test]
    fn a_scale_of_zero_or_nan_is_refused() {
        let zero = ResidualSettings {
            scale: 0.0,
            ..settings(false, false)
        };
        let nan = ResidualSettings {
            scale: f32::NAN,
            ..settings(false, false)
        };
        assert!(ResidualPd::new(repo_pd(), zero).is_err());
        assert!(ResidualPd::new(repo_pd(), nan).is_err());
    }

    #[test]
    fn a_force_scaled_nudge_in_arcade_is_half_the_throttle_of_realistic() {
        let residual = ResidualPd::new(repo_pd(), settings(true, false)).unwrap();

        let realistic = residual.nudge_scale(1.0, 0.0);
        let arcade = residual.nudge_scale(2.0, 0.0);

        assert!((realistic - 0.5).abs() < 1e-6);
        assert!((arcade - 0.25).abs() < 1e-6);
    }

    #[test]
    fn without_force_scaling_the_nudge_ignores_the_mode() {
        let residual = ResidualPd::new(repo_pd(), settings(false, false)).unwrap();

        assert_eq!(residual.nudge_scale(2.0, -1.0), 0.5);
    }

    #[test]
    fn a_closed_gate_gives_pd_whatever_the_nudges() {
        let mut pd = repo_pd();
        let mut residual = ResidualPd::new(repo_pd(), settings(false, true)).unwrap();
        let ctx = tumbling_ctx();
        let mut action = [1.0; RESIDUAL_ACTION_COUNT];
        action[NOZZLE_COUNT] = -1.0;

        let pd_throttles = pd.act(&ctx);
        let residual_throttles = residual.act(&ctx, action);

        assert_eq!(residual_throttles, pd_throttles);
    }

    #[test]
    fn the_gate_opens_from_zero_to_one_and_closes_on_nan() {
        assert_eq!(gate_opening(-1.0), 0.0);
        assert_eq!(gate_opening(0.0), 0.5);
        assert_eq!(gate_opening(1.0), 1.0);
        assert_eq!(gate_opening(3.0), 1.0);
        assert_eq!(gate_opening(f32::NAN), 0.0);
    }
}
