//! Residual control (M7 approach 2): PD flies the car and a policy only nudges it. Each
//! control step the throttles are PD's plus `scale` × the policy's action, clamped to
//! 0..1, so a zero action is exactly PD. Training (`nl-py`) uses it per environment; the
//! game would run the same code with the loaded policy.

use nl_sim::conventions::NOZZLE_COUNT;

use crate::{ControlCtx, Controller, Pd};

/// PD plus a scaled residual from a policy.
#[derive(Debug, Clone, PartialEq)]
pub struct ResidualPd {
    pd: Pd,
    /// Throttle change per unit of action: an action in −1..1 moves a throttle by up
    /// to ± this much from PD's.
    scale: f32,
}

impl ResidualPd {
    /// PD (already built for the car) with residual actions scaled by `scale`
    /// (throttle per unit action; must be finite and above zero).
    pub fn new(pd: Pd, scale: f32) -> Result<ResidualPd, String> {
        if !(scale.is_finite() && scale > 0.0) {
            return Err(format!(
                "residual scale must be a number above 0, not {scale}"
            ));
        }
        Ok(ResidualPd { pd, scale })
    }

    /// Forgets PD's warm start, at the start of an episode.
    pub fn reset(&mut self, seed: u64) {
        self.pd.reset(seed);
    }

    /// The throttles for this control step: PD's for `ctx` plus the scaled `action`.
    pub fn act(&mut self, ctx: &ControlCtx, action: [f32; NOZZLE_COUNT]) -> [f32; NOZZLE_COUNT] {
        let pd_throttles = self.pd.act(ctx);
        add_residual(pd_throttles, action, self.scale)
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
        let mut residual = ResidualPd::new(repo_pd(), 0.5).unwrap();
        let ctx = tumbling_ctx();

        let pd_throttles = pd.act(&ctx);
        let residual_throttles = residual.act(&ctx, [0.0; NOZZLE_COUNT]);

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
        assert!(ResidualPd::new(repo_pd(), 0.0).is_err());
        assert!(ResidualPd::new(repo_pd(), f32::NAN).is_err());
    }
}
