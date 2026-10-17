//! The car's settings, read from `configs/vehicle.toml`.
//!
//! The browser and Python both pass the TOML text in as a string, so this module
//! parses from a `&str` and never touches the file system. After parsing it checks
//! that every number makes physical sense, so a typo fails loudly at load time
//! instead of producing a strange simulation later.

use serde::{Deserialize, Serialize};

use crate::conventions::WHEEL_COUNT;

/// The text of the repo's `configs/vehicle.toml`, built into the library so tests,
/// examples, Python and the browser can all start from the same car.
pub const DEFAULT_VEHICLE_TOML: &str = include_str!("../../../configs/vehicle.toml");

/// Everything `configs/vehicle.toml` describes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VehicleConfig {
    pub chassis: ChassisConfig,
    pub air_drag: AirDragConfig,
    pub wheels: WheelConfig,
    pub drive: DriveConfig,
    pub steering: SteeringConfig,
}

/// The car body. See `[chassis]` in `configs/vehicle.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChassisConfig {
    /// Mass without air in the tank.
    pub dry_mass_kg: f32,
    /// Half-size of the main body box: half length (x), half height (y), half width (z).
    pub half_extents_m: [f32; 3],
    /// Half-size of the thin roof box that sits on top of the body.
    pub roof_half_extents_m: [f32; 3],
    /// Moments of inertia about the body axes: roll (x), yaw (y), pitch (z).
    pub inertia_kg_m2: [f32; 3],
}

/// Air resistance on the body. See `[air_drag]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirDragConfig {
    pub air_density_kg_m3: f32,
    /// Drag coefficient times frontal area (CdA).
    pub drag_area_m2: f32,
}

/// The four raycast wheels and their suspension. See `[wheels]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WheelConfig {
    /// Suspension attachment points in the body frame, in wheel order FL, FR, RL, RR.
    pub mount_points_m: [[f32; 3]; WHEEL_COUNT],
    pub radius_m: f32,
    pub suspension_rest_length_m: f32,
    /// How far the suspension can move either side of its rest length.
    pub suspension_max_travel_m: f32,
    /// Spring strength. Rapier multiplies it by the car's mass, so it is an
    /// acceleration per metre of compression ((m/s²)/m).
    pub suspension_stiffness_per_kg: f32,
    /// Damping while the spring compresses, also multiplied by mass ((m/s²)/(m/s)).
    pub suspension_compression_damping_per_kg: f32,
    /// Damping while the spring extends, also multiplied by mass ((m/s²)/(m/s)).
    pub suspension_relaxation_damping_per_kg: f32,
    /// Upper limit on one wheel's suspension force.
    pub max_suspension_force_n: f32,
    /// Tyre grip: how much of the suspension force can become a forward push before
    /// the tyre slips. Higher means stronger braking and acceleration.
    pub friction_slip: f32,
    /// Multiplier on sideways tyre grip. 1.0 is normal; 0.0 slides like ice.
    pub side_friction_stiffness: f32,
}

/// Engine and brakes. See `[drive]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriveConfig {
    pub max_engine_force_per_wheel_n: f32,
    /// The engine stops pushing forward at this speed.
    pub max_speed_m_s: f32,
    /// The engine stops pushing backward at this speed.
    pub max_reverse_speed_m_s: f32,
    /// Brake impulse per wheel per physics step.
    pub brake_impulse_per_wheel_n_s: f32,
    /// Brake impulse per wheel per physics step when no pedal is pressed.
    pub rolling_resistance_impulse_per_wheel_n_s: f32,
    /// Below this forward speed, a negative throttle reverses instead of braking.
    pub reverse_below_speed_m_s: f32,
}

/// Front-wheel steering limits. See `[steering]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SteeringConfig {
    /// Steering limit when standing still.
    pub max_angle_low_speed_rad: f32,
    /// Steering limit at `high_speed_m_s` and above.
    pub max_angle_high_speed_rad: f32,
    pub high_speed_m_s: f32,
    /// How fast the front wheels can turn.
    pub max_rate_rad_s: f32,
}

/// Why a vehicle config could not be used.
#[derive(Debug, thiserror::Error)]
pub enum VehicleConfigError {
    #[error("vehicle.toml could not be read: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("vehicle.toml: `{field}` must be a finite number greater than zero, but it is {value}")]
    NotPositive { field: &'static str, value: f32 },

    #[error("vehicle.toml: `{field}` must be a finite number, zero or more, but it is {value}")]
    Negative { field: &'static str, value: f32 },

    #[error(
        "vehicle.toml: `steering.max_angle_high_speed_rad` ({high}) must not be larger than \
         `steering.max_angle_low_speed_rad` ({low})"
    )]
    SteeringLimitsReversed { low: f32, high: f32 },
}

/// Short alias so the check functions below fit on one line each.
type CheckResult = Result<(), VehicleConfigError>;

impl VehicleConfig {
    /// Parses and checks the text of a `vehicle.toml` file.
    pub fn from_toml_str(text: &str) -> Result<Self, VehicleConfigError> {
        let config: VehicleConfig = toml::from_str(text)?;
        config.check()?;
        Ok(config)
    }

    /// Checks that every number is physically sensible.
    fn check(&self) -> CheckResult {
        self.chassis.check()?;
        self.air_drag.check()?;
        self.wheels.check()?;
        self.drive.check()?;
        self.steering.check()
    }
}

impl ChassisConfig {
    fn check(&self) -> CheckResult {
        must_be_positive("chassis.dry_mass_kg", self.dry_mass_kg)?;
        all_must_be_positive("chassis.half_extents_m", self.half_extents_m)?;
        all_must_be_positive("chassis.roof_half_extents_m", self.roof_half_extents_m)?;
        all_must_be_positive("chassis.inertia_kg_m2", self.inertia_kg_m2)
    }
}

impl AirDragConfig {
    fn check(&self) -> CheckResult {
        must_not_be_negative("air_drag.air_density_kg_m3", self.air_density_kg_m3)?;
        must_not_be_negative("air_drag.drag_area_m2", self.drag_area_m2)
    }
}

impl WheelConfig {
    fn check(&self) -> CheckResult {
        must_be_positive("wheels.radius_m", self.radius_m)?;
        must_be_positive(
            "wheels.suspension_rest_length_m",
            self.suspension_rest_length_m,
        )?;
        must_be_positive(
            "wheels.suspension_max_travel_m",
            self.suspension_max_travel_m,
        )?;
        must_be_positive(
            "wheels.suspension_stiffness_per_kg",
            self.suspension_stiffness_per_kg,
        )?;
        let compression = self.suspension_compression_damping_per_kg;
        must_not_be_negative("wheels.suspension_compression_damping_per_kg", compression)?;
        let relaxation = self.suspension_relaxation_damping_per_kg;
        must_not_be_negative("wheels.suspension_relaxation_damping_per_kg", relaxation)?;
        must_be_positive("wheels.max_suspension_force_n", self.max_suspension_force_n)?;
        must_be_positive("wheels.friction_slip", self.friction_slip)?;
        must_not_be_negative(
            "wheels.side_friction_stiffness",
            self.side_friction_stiffness,
        )
    }
}

impl DriveConfig {
    fn check(&self) -> CheckResult {
        let engine_force_n = self.max_engine_force_per_wheel_n;
        must_not_be_negative("drive.max_engine_force_per_wheel_n", engine_force_n)?;
        must_be_positive("drive.max_speed_m_s", self.max_speed_m_s)?;
        must_be_positive("drive.max_reverse_speed_m_s", self.max_reverse_speed_m_s)?;
        let brake_n_s = self.brake_impulse_per_wheel_n_s;
        must_not_be_negative("drive.brake_impulse_per_wheel_n_s", brake_n_s)?;
        let rolling_n_s = self.rolling_resistance_impulse_per_wheel_n_s;
        must_not_be_negative(
            "drive.rolling_resistance_impulse_per_wheel_n_s",
            rolling_n_s,
        )?;
        let reverse_m_s = self.reverse_below_speed_m_s;
        must_not_be_negative("drive.reverse_below_speed_m_s", reverse_m_s)
    }
}

impl SteeringConfig {
    fn check(&self) -> CheckResult {
        let low = self.max_angle_low_speed_rad;
        let high = self.max_angle_high_speed_rad;
        must_be_positive("steering.max_angle_low_speed_rad", low)?;
        must_not_be_negative("steering.max_angle_high_speed_rad", high)?;
        must_be_positive("steering.high_speed_m_s", self.high_speed_m_s)?;
        must_be_positive("steering.max_rate_rad_s", self.max_rate_rad_s)?;
        if high > low {
            return Err(VehicleConfigError::SteeringLimitsReversed { low, high });
        }
        Ok(())
    }
}

/// Fails if `value` is zero, negative, infinite or not a number.
fn must_be_positive(field: &'static str, value: f32) -> CheckResult {
    if !value.is_finite() || value <= 0.0 {
        return Err(VehicleConfigError::NotPositive { field, value });
    }
    Ok(())
}

/// Fails if any of the three values is zero, negative, infinite or not a number.
fn all_must_be_positive(field: &'static str, values: [f32; 3]) -> CheckResult {
    for value in values {
        must_be_positive(field, value)?;
    }
    Ok(())
}

/// Fails if `value` is negative, infinite or not a number.
fn must_not_be_negative(field: &'static str, value: f32) -> CheckResult {
    if !value.is_finite() || value < 0.0 {
        return Err(VehicleConfigError::Negative { field, value });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO_VEHICLE_TOML: &str = DEFAULT_VEHICLE_TOML;

    #[test]
    fn the_repo_vehicle_config_loads() {
        let config = VehicleConfig::from_toml_str(REPO_VEHICLE_TOML).unwrap();

        assert_eq!(config.chassis.dry_mass_kg, 1800.0);
        assert_eq!(config.wheels.mount_points_m.len(), 4);
    }

    #[test]
    fn front_left_wheel_is_in_front_and_on_the_left() {
        let config = VehicleConfig::from_toml_str(REPO_VEHICLE_TOML).unwrap();
        let front_left = config.wheels.mount_points_m[0];

        assert!(front_left[0] > 0.0, "front means +X");
        assert!(front_left[2] < 0.0, "left means -Z");
    }

    #[test]
    fn a_negative_mass_is_rejected_with_the_field_name() {
        let broken = REPO_VEHICLE_TOML.replace("dry_mass_kg = 1800.0", "dry_mass_kg = -5.0");

        let error = VehicleConfig::from_toml_str(&broken).unwrap_err();

        assert!(error.to_string().contains("chassis.dry_mass_kg"));
    }

    #[test]
    fn an_infinite_speed_limit_is_rejected() {
        let broken = REPO_VEHICLE_TOML.replace("max_speed_m_s = 45.0", "max_speed_m_s = inf");

        let error = VehicleConfig::from_toml_str(&broken).unwrap_err();

        assert!(error.to_string().contains("drive.max_speed_m_s"));
    }

    #[test]
    fn an_unknown_field_is_rejected() {
        let broken = REPO_VEHICLE_TOML.replace("[drive]", "[drive]\nturbo = true");

        let error = VehicleConfig::from_toml_str(&broken).unwrap_err();

        assert!(error.to_string().contains("turbo"));
    }

    #[test]
    fn reversed_steering_limits_are_rejected() {
        let broken = REPO_VEHICLE_TOML.replace(
            "max_angle_high_speed_rad = 0.15",
            "max_angle_high_speed_rad = 0.9",
        );

        let error = VehicleConfig::from_toml_str(&broken).unwrap_err();

        assert!(matches!(
            error,
            VehicleConfigError::SteeringLimitsReversed { .. }
        ));
    }
}
