//! Loading `configs/controllers.toml` (SPEC §8): the PD gains and the allocation
//! settings. Every number is checked, and an error names the bad field.

use serde::Deserialize;

use crate::allocation::AllocationConfig;

/// The repo's `configs/controllers.toml`, built in so the game and tests share it.
pub const DEFAULT_CONTROLLERS_TOML: &str = include_str!("../../../configs/controllers.toml");

/// The PD controller's gains and thresholds (`[pd]`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdConfig {
    /// Attitude gain on the tilt error near upright (1/s²).
    pub kp: f32,
    /// Attitude damping on the spin near upright (1/s).
    pub kd: f32,
    /// Attitude gain once tilted past `tumble_blend_end_deg` (1/s²): a tumbling car needs
    /// a firmer push than a car finishing a jump, which a high gain unsettles.
    pub kp_tumble: f32,
    /// Attitude damping once tilted past `tumble_blend_end_deg` (1/s).
    pub kd_tumble: f32,
    /// Below this tilt (degrees) PD uses `kp`/`kd`; from here to `tumble_blend_end_deg`
    /// the gains slide linearly to `kp_tumble`/`kd_tumble`.
    pub tumble_blend_start_deg: f32,
    /// See `tumble_blend_start_deg`, in degrees; must be larger.
    pub tumble_blend_end_deg: f32,
    /// Braking only happens while the car's up axis points at least this far up.
    pub upright_alignment: f32,
    /// Braking always starts by this height (the floor of the stopping-distance rule).
    pub brake_height_m: f32,
    /// Braking starts this many times the stopping distance up, so a fast fall gets
    /// thrust early enough to slow down (owner's free-play feedback: hard landings).
    pub brake_safety: f32,
    /// The share of full lift the stopping distance counts on (the rest is headroom for
    /// attitude torque), 0..1.
    pub brake_usable_fraction: f32,
    /// Extra height added to the stopping distance: the centre of mass rides this far
    /// above the ground on landing, plus slack, in m.
    pub brake_margin_m: f32,
    /// ... and when falling faster than this.
    pub brake_speed_m_s: f32,
    /// The fall speed braking aims for.
    pub target_descent_m_s: f32,
    /// Braking gain on the fall-speed error (1/s).
    pub kv: f32,
    /// Below this height, not upright and slower than `lying_max_fall_speed_m_s`, the
    /// car is lying on the ground.
    pub on_ground_height_m: f32,
    /// See `on_ground_height_m`.
    pub lying_max_fall_speed_m_s: f32,
    /// Lying on the ground, the roll torque is multiplied by this: tipping the car over
    /// its side edge needs much more than turning it in the air.
    pub ground_roll_gain: f32,
    /// Lying on its side, the side boosters push the car up with this share of its weight
    /// (along whichever side axis points up), helping the roll torque flip it.
    pub ground_side_push: f32,
    /// In the air, sideways sliding is damped with this gain (1/s), so the side boosters
    /// straighten the car's path before touchdown.
    pub kv_lateral: f32,
    /// Cap on the angular acceleration PD asks for, in rad/s², so a sudden turn stays
    /// gentle on the passenger (about 3 g at the seat with the spin it allows).
    pub max_angular_accel_rad_s2: f32,
    /// Air control: full steer asks for this yaw rate, in rad/s.
    pub air_yaw_rate_rad_s: f32,
    /// Air control: full throttle or brake asks for this nose-down or nose-up pitch, in rad.
    pub air_pitch_rad: f32,
    /// Air control only above this height; lower, PD lines up for landing, in m.
    pub air_wish_min_height_m: f32,
    /// With any wheel down, tilted at most this much (degrees) and spinning at most
    /// `quiet_max_spin_rad_s`, PD treats the car as driving and leaves the thrusters off.
    pub quiet_max_tilt_deg: f32,
    /// See `quiet_max_tilt_deg`, in rad/s.
    pub quiet_max_spin_rad_s: f32,
}

/// The Hybrid's gate, `[hybrid]` in controllers.toml. Optional: left out, the gate uses
/// the car's upset and save thresholds (`HybridGateSettings::for_vehicle`).
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HybridConfig {
    /// The AI takes over only past this tilt, in degrees (0..=180); None uses the car's
    /// upset angle.
    pub enter_tilt_deg: Option<f32>,
}

/// Everything in controllers.toml.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllersConfig {
    pub pd: PdConfig,
    pub allocation: AllocationConfig,
    #[serde(default)]
    pub hybrid: HybridConfig,
}

/// Why controllers.toml could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ControllersConfigError {
    #[error("controllers.toml: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("controllers.toml: `{field}` must be a positive number, but it is {value}")]
    NotPositive { field: &'static str, value: f32 },

    #[error("controllers.toml: `{field}` must be between 0 and 1, but it is {value}")]
    NotFraction { field: &'static str, value: f32 },

    #[error(
        "controllers.toml: `{field}` must be above 0 and at most 180 degrees, but it is {value}"
    )]
    NotTilt { field: &'static str, value: f32 },
}

impl ControllersConfig {
    /// Parses and checks the text of a controllers.toml.
    pub fn from_toml_str(text: &str) -> Result<ControllersConfig, ControllersConfigError> {
        let config: ControllersConfig = toml::from_str(text)?;
        config.check()?;
        Ok(config)
    }

    fn check(&self) -> Result<(), ControllersConfigError> {
        let pd = &self.pd;
        let allocation = &self.allocation;
        let fields = [
            ("pd.kp", pd.kp),
            ("pd.kd", pd.kd),
            ("pd.kp_tumble", pd.kp_tumble),
            ("pd.kd_tumble", pd.kd_tumble),
            ("pd.tumble_blend_start_deg", pd.tumble_blend_start_deg),
            ("pd.tumble_blend_end_deg", pd.tumble_blend_end_deg),
            ("pd.upright_alignment", pd.upright_alignment),
            ("pd.brake_height_m", pd.brake_height_m),
            ("pd.brake_safety", pd.brake_safety),
            ("pd.brake_usable_fraction", pd.brake_usable_fraction),
            ("pd.brake_margin_m", pd.brake_margin_m),
            ("pd.brake_speed_m_s", pd.brake_speed_m_s),
            ("pd.target_descent_m_s", pd.target_descent_m_s),
            ("pd.kv", pd.kv),
            ("pd.on_ground_height_m", pd.on_ground_height_m),
            ("pd.lying_max_fall_speed_m_s", pd.lying_max_fall_speed_m_s),
            ("pd.ground_roll_gain", pd.ground_roll_gain),
            ("pd.ground_side_push", pd.ground_side_push),
            ("pd.kv_lateral", pd.kv_lateral),
            ("pd.max_angular_accel_rad_s2", pd.max_angular_accel_rad_s2),
            ("pd.air_yaw_rate_rad_s", pd.air_yaw_rate_rad_s),
            ("pd.air_pitch_rad", pd.air_pitch_rad),
            ("pd.air_wish_min_height_m", pd.air_wish_min_height_m),
            ("pd.quiet_max_tilt_deg", pd.quiet_max_tilt_deg),
            ("pd.quiet_max_spin_rad_s", pd.quiet_max_spin_rad_s),
            ("allocation.force_weight", allocation.force_weight),
            ("allocation.torque_weight", allocation.torque_weight),
            ("allocation.regularisation", allocation.regularisation),
            ("allocation.iterations", allocation.iterations as f32),
        ];
        for (field, value) in fields {
            if !value.is_finite() || value <= 0.0 {
                return Err(ControllersConfigError::NotPositive { field, value });
            }
        }
        // An alignment is the cosine of a tilt and the others are shares, so each must be
        // a fraction.
        let fractions = [
            ("pd.upright_alignment", pd.upright_alignment),
            ("pd.brake_usable_fraction", pd.brake_usable_fraction),
            ("pd.ground_side_push", pd.ground_side_push),
        ];
        if pd.tumble_blend_end_deg <= pd.tumble_blend_start_deg {
            return Err(ControllersConfigError::NotPositive {
                field: "pd.tumble_blend_end_deg - pd.tumble_blend_start_deg",
                value: pd.tumble_blend_end_deg - pd.tumble_blend_start_deg,
            });
        }
        let lag_compensation = allocation.lag_compensation;
        if !(0.0..=1.0).contains(&lag_compensation) {
            return Err(ControllersConfigError::NotFraction {
                field: "allocation.lag_compensation",
                value: lag_compensation,
            });
        }
        for (field, value) in fractions {
            if value > 1.0 {
                return Err(ControllersConfigError::NotFraction { field, value });
            }
        }
        if let Some(value) = self.hybrid.enter_tilt_deg
            && !(value > 0.0 && value <= 180.0)
        {
            return Err(ControllersConfigError::NotTilt {
                field: "hybrid.enter_tilt_deg",
                value,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_repo_config_loads() {
        let config = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();

        assert!(config.pd.kp > 0.0);
    }

    /// The repo's controllers.toml with the `name = …` line set to `name = value`.
    fn with_value(name: &str, value: &str) -> String {
        let prefix = format!("{name} = ");
        let mut lines: Vec<String> = Vec::new();
        for line in DEFAULT_CONTROLLERS_TOML.lines() {
            if line.starts_with(&prefix) {
                lines.push(format!("{prefix}{value}"));
            } else {
                lines.push(line.to_string());
            }
        }
        lines.join("\n")
    }

    #[test]
    fn a_negative_gain_is_refused_with_its_field_name() {
        let broken = with_value("kd", "-4.0");

        let error = ControllersConfig::from_toml_str(&broken).unwrap_err();

        assert_eq!(
            error.to_string(),
            "controllers.toml: `pd.kd` must be a positive number, but it is -4"
        );
    }

    #[test]
    fn an_upright_alignment_above_1_is_refused() {
        let broken = with_value("upright_alignment", "1.5");

        let error = ControllersConfig::from_toml_str(&broken).unwrap_err();

        assert!(error.to_string().contains("between 0 and 1"), "{error}");
    }
}
