//! The car's settings, read from `configs/vehicle.toml`.
//!
//! The browser and Python both pass the TOML text in as a string, so this module
//! parses from a `&str` and never touches the file system. After parsing it checks
//! that every number makes physical sense, so a typo fails loudly at load time
//! instead of producing a strange simulation later.

use serde::Deserialize;

use crate::conventions::WHEEL_COUNT;

/// Everything `configs/vehicle.toml` describes.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VehicleConfig {
    pub chassis: ChassisConfig,
    pub air_drag: AirDragConfig,
    pub wheels: WheelConfig,
    pub drive: DriveConfig,
    pub steering: SteeringConfig,
}

/// The car body. See `[chassis]` in `configs/vehicle.toml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChassisConfig {
    pub dry_mass_kg: f32,
    pub half_extents_m: [f32; 3],
    pub roof_half_extents_m: [f32; 3],
    pub inertia_kg_m2: [f32; 3],
}

/// Air resistance on the body. See `[air_drag]`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirDragConfig {
    pub air_density_kg_m3: f32,
    pub drag_area_m2: f32,
}

/// The four raycast wheels and their suspension. See `[wheels]`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WheelConfig {
    pub mount_points_m: [[f32; 3]; WHEEL_COUNT],
    pub radius_m: f32,
    pub suspension_rest_length_m: f32,
    pub suspension_max_travel_m: f32,
    pub suspension_stiffness_per_kg: f32,
    pub suspension_compression_damping_per_kg: f32,
    pub suspension_relaxation_damping_per_kg: f32,
    pub max_suspension_force_n: f32,
    pub friction_slip: f32,
    pub side_friction_stiffness: f32,
}

/// Engine and brakes. See `[drive]`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriveConfig {
    pub max_engine_force_per_wheel_n: f32,
    pub max_speed_m_s: f32,
    pub brake_impulse_per_wheel_n_s: f32,
}

/// Front-wheel steering limits. See `[steering]`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SteeringConfig {
    pub max_angle_low_speed_rad: f32,
    pub max_angle_high_speed_rad: f32,
    pub high_speed_m_s: f32,
    pub max_rate_rad_s: f32,
}

/// Why a vehicle config could not be used.
#[derive(Debug, thiserror::Error)]
pub enum VehicleConfigError {
    #[error("vehicle.toml could not be read: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("vehicle.toml: `{field}` must be greater than zero, but it is {value}")]
    NotPositive { field: &'static str, value: f32 },

    #[error("vehicle.toml: `{field}` must be zero or more, but it is {value}")]
    Negative { field: &'static str, value: f32 },

    #[error(
        "vehicle.toml: `steering.max_angle_high_speed_rad` ({high}) must not be larger than \
         `steering.max_angle_low_speed_rad` ({low})"
    )]
    SteeringLimitsReversed { low: f32, high: f32 },
}

impl VehicleConfig {
    /// Parses and checks the text of a `vehicle.toml` file.
    pub fn from_toml_str(text: &str) -> Result<Self, VehicleConfigError> {
        let config: VehicleConfig = toml::from_str(text)?;
        config.check()?;
        Ok(config)
    }

    /// Checks that every number is physically sensible.
    fn check(&self) -> Result<(), VehicleConfigError> {
        let chassis = &self.chassis;
        must_be_positive("chassis.dry_mass_kg", chassis.dry_mass_kg)?;
        for value in chassis.half_extents_m {
            must_be_positive("chassis.half_extents_m", value)?;
        }
        for value in chassis.roof_half_extents_m {
            must_be_positive("chassis.roof_half_extents_m", value)?;
        }
        for value in chassis.inertia_kg_m2 {
            must_be_positive("chassis.inertia_kg_m2", value)?;
        }

        must_not_be_negative(
            "air_drag.air_density_kg_m3",
            self.air_drag.air_density_kg_m3,
        )?;
        must_not_be_negative("air_drag.drag_area_m2", self.air_drag.drag_area_m2)?;

        let wheels = &self.wheels;
        must_be_positive("wheels.radius_m", wheels.radius_m)?;
        must_be_positive(
            "wheels.suspension_rest_length_m",
            wheels.suspension_rest_length_m,
        )?;
        must_be_positive(
            "wheels.suspension_max_travel_m",
            wheels.suspension_max_travel_m,
        )?;
        must_be_positive(
            "wheels.suspension_stiffness_per_kg",
            wheels.suspension_stiffness_per_kg,
        )?;
        must_not_be_negative(
            "wheels.suspension_compression_damping_per_kg",
            wheels.suspension_compression_damping_per_kg,
        )?;
        must_not_be_negative(
            "wheels.suspension_relaxation_damping_per_kg",
            wheels.suspension_relaxation_damping_per_kg,
        )?;
        must_be_positive(
            "wheels.max_suspension_force_n",
            wheels.max_suspension_force_n,
        )?;
        must_be_positive("wheels.friction_slip", wheels.friction_slip)?;
        must_not_be_negative(
            "wheels.side_friction_stiffness",
            wheels.side_friction_stiffness,
        )?;

        let drive = &self.drive;
        must_not_be_negative(
            "drive.max_engine_force_per_wheel_n",
            drive.max_engine_force_per_wheel_n,
        )?;
        must_be_positive("drive.max_speed_m_s", drive.max_speed_m_s)?;
        must_not_be_negative(
            "drive.brake_impulse_per_wheel_n_s",
            drive.brake_impulse_per_wheel_n_s,
        )?;

        let steering = &self.steering;
        must_be_positive(
            "steering.max_angle_low_speed_rad",
            steering.max_angle_low_speed_rad,
        )?;
        must_not_be_negative(
            "steering.max_angle_high_speed_rad",
            steering.max_angle_high_speed_rad,
        )?;
        must_be_positive("steering.high_speed_m_s", steering.high_speed_m_s)?;
        must_be_positive("steering.max_rate_rad_s", steering.max_rate_rad_s)?;
        if steering.max_angle_high_speed_rad > steering.max_angle_low_speed_rad {
            return Err(VehicleConfigError::SteeringLimitsReversed {
                low: steering.max_angle_low_speed_rad,
                high: steering.max_angle_high_speed_rad,
            });
        }

        Ok(())
    }
}

/// Fails if `value` is zero, negative or not a number.
fn must_be_positive(field: &'static str, value: f32) -> Result<(), VehicleConfigError> {
    if value.is_nan() || value <= 0.0 {
        return Err(VehicleConfigError::NotPositive { field, value });
    }
    Ok(())
}

/// Fails if `value` is negative or not a number.
fn must_not_be_negative(field: &'static str, value: f32) -> Result<(), VehicleConfigError> {
    if value.is_nan() || value < 0.0 {
        return Err(VehicleConfigError::Negative { field, value });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real config file shipped in the repo.
    const REPO_VEHICLE_TOML: &str = include_str!("../../../configs/vehicle.toml");

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
