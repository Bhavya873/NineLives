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
    /// Landing on an obstacle top, the brake height grows by this much: the nose can
    /// overhang the edge and meet it first, in m.
    pub obstacle_margin_m: f32,
    /// PD wants the car settled and braking this long before touchdown, in s.
    pub catch_window_s: f32,
    /// Limited air, touchdown still far (more than the braking time plus `catch_window_s`
    /// away): PD leaves a tilt below this alone, in degrees, to save air.
    pub hold_deadband_deg: f32,
    /// ... and a spin below this, in rad/s.
    pub hold_max_spin_rad_s: f32,
    /// The landing air reserve counts this many times the air the final braking needs.
    pub reserve_safety: f32,
    /// ... plus the air the last catch needs, in kg.
    pub reserve_catch_air_kg: f32,
    /// Limited air, below this much air the attitude gains slide down towards
    /// `low_air_gain_share` (not a cliff), in kg.
    pub low_air_floor_kg: f32,
    /// The share of the attitude gains left with an empty tank, 0..1.
    pub low_air_gain_share: f32,
    /// Extra lateral damping at an edge: the gain is multiplied by 1 + this × how close
    /// the edge is (0 at `EDGE_FAR_M` or more, 1 at the edge).
    pub edge_damping_boost: f32,
    /// Landing on a short surface, PD tips the nose up or down by this much (rad) per
    /// m/s of forward speed to slow the slide, up to `edge_pitch_max_rad`.
    pub edge_pitch_rad_per_m_s: f32,
    /// See `edge_pitch_rad_per_m_s`, in rad.
    pub edge_pitch_max_rad: f32,
    /// The catch aims the car's up axis at the surface normal when it is tilted at most
    /// this much from level, in degrees.
    pub normal_aim_max_deg: f32,
}

/// The Hybrid's gate, `[hybrid]` in controllers.toml. Optional: left out, the gate uses
/// the car's upset and save thresholds (`HybridGateSettings::for_vehicle`).
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HybridConfig {
    /// The AI takes over only past this tilt, in degrees (0..=180); None uses the car's
    /// upset angle.
    pub enter_tilt_deg: Option<f32>,
    /// The policy counts as validated up to this speed, in m/s; faster, PD flies (the
    /// policy envelope). None uses 55, the speed it was evaluated to. Raise it only after
    /// running the Hybrid eval at the higher speed and logging it in docs/EXPERIMENTS.md.
    pub policy_max_speed_m_s: Option<f32>,
}

/// The stunt planner's settings, `[stunt]` in controllers.toml (docs/contracts/stunt-states.md,
/// numbers from docs/research/stunt-planning.md). Optional: left out, these defaults apply.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct StuntConfig {
    /// Coast rate of a flip in Realistic, rad/s.
    pub flip_coast_rate_realistic_rad_s: f32,
    /// Coast rate of a barrel roll in Realistic, rad/s.
    pub barrel_coast_rate_realistic_rad_s: f32,
    /// Coast rate of a flip in Arcade, rad/s.
    pub flip_coast_rate_arcade_rad_s: f32,
    /// Coast rate of a barrel roll in Arcade, rad/s.
    pub barrel_coast_rate_arcade_rad_s: f32,
    /// Angular acceleration about the pitch axis through the allocator at thrust scale 1
    /// and a full tank, rad/s².
    pub pitch_alpha_rad_s2: f32,
    /// The same about the roll axis, rad/s².
    pub roll_alpha_rad_s2: f32,
    /// Air used while firing full pitch torque at thrust scale 1, kg/s.
    pub pitch_air_kg_s: f32,
    /// The same for full roll torque, kg/s.
    pub roll_air_kg_s: f32,
    /// The Brake phase hands over to PD at this spin, rad/s.
    pub handover_rate_rad_s: f32,
    /// Time PD needs after the hand-over to make the car landable, s.
    pub catch_time_s: f32,
    /// Air PD needs for that, kg.
    pub catch_air_kg: f32,
    /// Rotation short of 2π still counted as complete, rad.
    pub complete_rotation_slack_rad: f32,
    /// Spare time on top of spin, brake and catch, s.
    pub settle_margin_s: f32,
    /// The landing surface may tilt this far from level, degrees.
    pub landing_max_normal_tilt_deg: f32,
    /// The usable landing length must be at least this many car lengths.
    pub landing_min_length_car_lengths: f32,
    /// Car length used for the landing check, m.
    pub car_length_m: f32,
    /// Relative speed spread of the forecast's uncertainty samples (0.02 = 2%).
    pub forecast_speed_spread: f32,
    /// Pitch spread of those samples, degrees.
    pub forecast_pitch_spread_deg: f32,
    /// Safety factor on the air a stunt needs.
    pub air_margin: f32,
    /// Air kept for the PD landing per m/s of fall speed at touchdown (from the forecast),
    /// kg: mass × (1 + g ÷ braking deceleration) ÷ exhaust speed, with some margin.
    pub landing_air_kg_per_m_s: f32,
    /// A stunt may only start at most this tilted, degrees.
    pub start_max_tilt_deg: f32,
    /// ... and spinning at most this fast, rad/s.
    pub start_max_spin_rad_s: f32,
    /// A queued request expires after this long, s.
    pub request_timeout_s: f32,
    /// The Hybrid gate stays on PD this long after Settled, s.
    pub hybrid_hold_after_settled_s: f32,
}

impl Default for StuntConfig {
    fn default() -> StuntConfig {
        StuntConfig {
            flip_coast_rate_realistic_rad_s: 5.0,
            barrel_coast_rate_realistic_rad_s: 5.0,
            flip_coast_rate_arcade_rad_s: 5.0,
            barrel_coast_rate_arcade_rad_s: 6.0,
            pitch_alpha_rad_s2: 13.2,
            roll_alpha_rad_s2: 28.7,
            pitch_air_kg_s: 40.0,
            roll_air_kg_s: 53.3,
            handover_rate_rad_s: 1.5,
            catch_time_s: 0.8,
            catch_air_kg: 11.0,
            complete_rotation_slack_rad: 0.15,
            settle_margin_s: 0.3,
            landing_max_normal_tilt_deg: 15.0,
            landing_min_length_car_lengths: 2.0,
            car_length_m: 4.2,
            forecast_speed_spread: 0.02,
            forecast_pitch_spread_deg: 1.0,
            air_margin: 1.25,
            landing_air_kg_per_m_s: 6.0,
            start_max_tilt_deg: 25.0,
            start_max_spin_rad_s: 1.5,
            request_timeout_s: 3.0,
            hybrid_hold_after_settled_s: 1.0,
        }
    }
}

impl StuntConfig {
    /// Every number as (name, value) for the positive check.
    fn fields(&self) -> [(&'static str, f32); 24] {
        [
            (
                "stunt.flip_coast_rate_realistic_rad_s",
                self.flip_coast_rate_realistic_rad_s,
            ),
            (
                "stunt.barrel_coast_rate_realistic_rad_s",
                self.barrel_coast_rate_realistic_rad_s,
            ),
            (
                "stunt.flip_coast_rate_arcade_rad_s",
                self.flip_coast_rate_arcade_rad_s,
            ),
            (
                "stunt.barrel_coast_rate_arcade_rad_s",
                self.barrel_coast_rate_arcade_rad_s,
            ),
            ("stunt.pitch_alpha_rad_s2", self.pitch_alpha_rad_s2),
            ("stunt.roll_alpha_rad_s2", self.roll_alpha_rad_s2),
            ("stunt.pitch_air_kg_s", self.pitch_air_kg_s),
            ("stunt.roll_air_kg_s", self.roll_air_kg_s),
            ("stunt.handover_rate_rad_s", self.handover_rate_rad_s),
            ("stunt.catch_time_s", self.catch_time_s),
            ("stunt.catch_air_kg", self.catch_air_kg),
            (
                "stunt.complete_rotation_slack_rad",
                self.complete_rotation_slack_rad,
            ),
            ("stunt.settle_margin_s", self.settle_margin_s),
            (
                "stunt.landing_max_normal_tilt_deg",
                self.landing_max_normal_tilt_deg,
            ),
            (
                "stunt.landing_min_length_car_lengths",
                self.landing_min_length_car_lengths,
            ),
            ("stunt.car_length_m", self.car_length_m),
            ("stunt.forecast_speed_spread", self.forecast_speed_spread),
            (
                "stunt.forecast_pitch_spread_deg",
                self.forecast_pitch_spread_deg,
            ),
            ("stunt.air_margin", self.air_margin),
            ("stunt.landing_air_kg_per_m_s", self.landing_air_kg_per_m_s),
            ("stunt.start_max_tilt_deg", self.start_max_tilt_deg),
            ("stunt.start_max_spin_rad_s", self.start_max_spin_rad_s),
            ("stunt.request_timeout_s", self.request_timeout_s),
            (
                "stunt.hybrid_hold_after_settled_s",
                self.hybrid_hold_after_settled_s,
            ),
        ]
    }
}

/// Everything in controllers.toml.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllersConfig {
    pub pd: PdConfig,
    pub allocation: AllocationConfig,
    #[serde(default)]
    pub hybrid: HybridConfig,
    #[serde(default)]
    pub stunt: StuntConfig,
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
            ("pd.obstacle_margin_m", pd.obstacle_margin_m),
            ("pd.catch_window_s", pd.catch_window_s),
            ("pd.hold_deadband_deg", pd.hold_deadband_deg),
            ("pd.hold_max_spin_rad_s", pd.hold_max_spin_rad_s),
            ("pd.reserve_safety", pd.reserve_safety),
            ("pd.reserve_catch_air_kg", pd.reserve_catch_air_kg),
            ("pd.low_air_floor_kg", pd.low_air_floor_kg),
            ("pd.edge_pitch_max_rad", pd.edge_pitch_max_rad),
            ("pd.normal_aim_max_deg", pd.normal_aim_max_deg),
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
        for (field, value) in self.stunt.fields() {
            if !value.is_finite() || value <= 0.0 {
                return Err(ControllersConfigError::NotPositive { field, value });
            }
        }
        let stunt = &self.stunt;
        if stunt.landing_max_normal_tilt_deg >= 90.0 {
            return Err(ControllersConfigError::NotTilt {
                field: "stunt.landing_max_normal_tilt_deg",
                value: stunt.landing_max_normal_tilt_deg,
            });
        }
        if stunt.start_max_tilt_deg > 180.0 {
            return Err(ControllersConfigError::NotTilt {
                field: "stunt.start_max_tilt_deg",
                value: stunt.start_max_tilt_deg,
            });
        }
        if stunt.forecast_speed_spread >= 1.0 {
            return Err(ControllersConfigError::NotFraction {
                field: "stunt.forecast_speed_spread",
                value: stunt.forecast_speed_spread,
            });
        }
        if stunt.complete_rotation_slack_rad >= std::f32::consts::PI {
            return Err(ControllersConfigError::NotFraction {
                field: "stunt.complete_rotation_slack_rad",
                value: stunt.complete_rotation_slack_rad,
            });
        }
        if let Some(value) = self.hybrid.enter_tilt_deg
            && !(value > 0.0 && value <= 180.0)
        {
            return Err(ControllersConfigError::NotTilt {
                field: "hybrid.enter_tilt_deg",
                value,
            });
        }
        if let Some(value) = self.hybrid.policy_max_speed_m_s
            && !(value.is_finite() && value > 0.0)
        {
            return Err(ControllersConfigError::NotPositive {
                field: "hybrid.policy_max_speed_m_s",
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

    #[test]
    fn the_policy_speed_limit_is_optional_and_checked() {
        let default = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();
        assert_eq!(default.hybrid.policy_max_speed_m_s, None);

        let raised = format!("{DEFAULT_CONTROLLERS_TOML}\npolicy_max_speed_m_s = 70.0\n");
        let config = ControllersConfig::from_toml_str(&raised).unwrap();
        assert_eq!(config.hybrid.policy_max_speed_m_s, Some(70.0));

        let broken = format!("{DEFAULT_CONTROLLERS_TOML}\npolicy_max_speed_m_s = -1.0\n");
        let error = ControllersConfig::from_toml_str(&broken).unwrap_err();
        assert_eq!(
            error.to_string(),
            "controllers.toml: `hybrid.policy_max_speed_m_s` must be a positive number, but it is -1"
        );
    }

    #[test]
    fn the_repo_stunt_table_is_the_default() {
        let config = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();
        assert_eq!(config.stunt, StuntConfig::default());
    }

    #[test]
    fn an_unknown_stunt_key_is_refused() {
        let broken =
            DEFAULT_CONTROLLERS_TOML.replace("[stunt]\n", "[stunt]\nbogus_setting = 1.0\n");
        let error = ControllersConfig::from_toml_str(&broken).unwrap_err();
        assert!(error.to_string().contains("bogus_setting"), "{error}");
    }

    #[test]
    fn an_unknown_table_is_refused() {
        let broken = format!("{DEFAULT_CONTROLLERS_TOML}\n[bogus]\nx = 1\n");
        assert!(ControllersConfig::from_toml_str(&broken).is_err());
    }

    #[test]
    fn a_bad_stunt_number_names_its_field() {
        let broken = with_value("catch_time_s", "-0.5");
        let error = ControllersConfig::from_toml_str(&broken).unwrap_err();
        assert_eq!(
            error.to_string(),
            "controllers.toml: `stunt.catch_time_s` must be a positive number, but it is -0.5"
        );
    }

    #[test]
    fn the_stunt_table_is_optional() {
        let cut = DEFAULT_CONTROLLERS_TOML
            .split("[stunt]")
            .next()
            .unwrap()
            .to_string()
            + "[hybrid]\n";
        let config = ControllersConfig::from_toml_str(&cut).unwrap();
        assert_eq!(config.stunt, StuntConfig::default());
    }
}
