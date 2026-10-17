//! The car's settings, read from `configs/vehicle.toml`.
//!
//! The browser and Python both pass the TOML text in as a string, so this module
//! parses from a `&str` and never touches the file system. After parsing it checks
//! that every number makes physical sense, so a typo fails loudly at load time
//! instead of producing a strange simulation later.

use serde::{Deserialize, Serialize};

use crate::conventions::{NOZZLE_COUNT, NOZZLE_NAMES, WHEEL_COUNT};

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
    pub thrusters: ThrusterConfig,
    pub events: EventConfig,
    pub chaos: ChaosConfig,
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

/// The cold-gas thrusters. See `[thrusters]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThrusterConfig {
    /// Time constant of the first-order valve lag (SPEC §7.4).
    pub valve_time_constant_s: f32,
    /// Speed of the cold-gas exhaust; air used = thrust / this speed (SPEC §7.5).
    pub exhaust_speed_m_s: f32,
    /// The nozzles, in the fixed `NOZZLE_NAMES` order.
    pub nozzles: [NozzleConfig; NOZZLE_COUNT],
}

/// When upsets, saves and crashes happen (SPEC §7.9). See `[events]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventConfig {
    /// An upset starts after this long with no wheel on the ground.
    pub upset_airborne_s: f32,
    /// ...or when the car tilts past this.
    pub upset_tilt_deg: f32,
    /// A save needs the car within this tilt of level...
    pub save_tilt_deg: f32,
    /// ...turning slower than this...
    pub save_max_spin_rad_s: f32,
    /// ...for this long, on all 4 wheels.
    pub save_hold_s: f32,
    /// Crash if the body or roof hits something faster than this along the contact normal.
    pub crash_impact_speed_m_s: f32,
    /// Crash if the roof touches anything while tilted past this.
    pub upside_down_tilt_deg: f32,
    /// Crash if tilted past this...
    pub stuck_tilt_deg: f32,
    /// ...for longer than this.
    pub stuck_s: f32,
    /// Crash if the car falls below this height.
    pub out_of_bounds_y_m: f32,
}

/// The size of a Chaos knock (SPEC §7.12). See `[chaos]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChaosConfig {
    /// Largest random push.
    pub max_linear_impulse_n_s: f32,
    /// Largest random twist.
    pub max_angular_impulse_n_m_s: f32,
}

/// One nozzle. See `[[thrusters.nozzles]]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NozzleConfig {
    /// Must match `NOZZLE_NAMES` at the same index, e.g. "L_FL".
    pub name: String,
    /// Where the nozzle is, relative to the centre of mass, in the body frame.
    pub position_m: [f32; 3],
    /// Unit vector: the direction the nozzle pushes the car (opposite to the exhaust).
    pub direction: [f32; 3],
    /// Push at full throttle in Realistic mode.
    pub max_force_n: f32,
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

    #[error("vehicle.toml: nozzle {index} must be named \"{expected}\", but it is \"{found}\"")]
    WrongNozzleOrder {
        index: usize,
        expected: &'static str,
        found: String,
    },

    #[error(
        "vehicle.toml: nozzle \"{name}\" direction must have length 1, but it has length {length}"
    )]
    DirectionNotUnitLength { name: String, length: f32 },

    #[error("vehicle.toml: nozzle \"{name}\": `{field}` is not valid ({reason})")]
    BadNozzleValue {
        name: String,
        field: &'static str,
        reason: &'static str,
    },
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
        self.steering.check()?;
        self.thrusters.check()?;
        self.events.check()?;
        self.chaos.check()
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

impl EventConfig {
    fn check(&self) -> CheckResult {
        must_be_positive("events.upset_airborne_s", self.upset_airborne_s)?;
        must_be_positive("events.upset_tilt_deg", self.upset_tilt_deg)?;
        must_be_positive("events.save_tilt_deg", self.save_tilt_deg)?;
        must_be_positive("events.save_max_spin_rad_s", self.save_max_spin_rad_s)?;
        must_not_be_negative("events.save_hold_s", self.save_hold_s)?;
        must_be_positive("events.crash_impact_speed_m_s", self.crash_impact_speed_m_s)?;
        must_be_positive("events.upside_down_tilt_deg", self.upside_down_tilt_deg)?;
        must_be_positive("events.stuck_tilt_deg", self.stuck_tilt_deg)?;
        must_not_be_negative("events.stuck_s", self.stuck_s)?;
        if !self.out_of_bounds_y_m.is_finite() {
            let value = self.out_of_bounds_y_m;
            return Err(VehicleConfigError::NotPositive {
                field: "events.out_of_bounds_y_m (must be finite)",
                value,
            });
        }
        Ok(())
    }
}

impl ChaosConfig {
    fn check(&self) -> CheckResult {
        must_not_be_negative("chaos.max_linear_impulse_n_s", self.max_linear_impulse_n_s)?;
        let angular = self.max_angular_impulse_n_m_s;
        must_not_be_negative("chaos.max_angular_impulse_n_m_s", angular)
    }
}

impl ThrusterConfig {
    fn check(&self) -> CheckResult {
        must_be_positive(
            "thrusters.valve_time_constant_s",
            self.valve_time_constant_s,
        )?;
        must_be_positive("thrusters.exhaust_speed_m_s", self.exhaust_speed_m_s)?;
        for (index, nozzle) in self.nozzles.iter().enumerate() {
            nozzle.check(index)?;
        }
        Ok(())
    }
}

impl NozzleConfig {
    fn check(&self, index: usize) -> CheckResult {
        let expected = NOZZLE_NAMES[index];
        if self.name != expected {
            return Err(VehicleConfigError::WrongNozzleOrder {
                index,
                expected,
                found: self.name.clone(),
            });
        }
        let [x, y, z] = self.direction;
        let length = (x * x + y * y + z * z).sqrt();
        if !length.is_finite() || (length - 1.0).abs() > 1e-3 {
            return Err(VehicleConfigError::DirectionNotUnitLength {
                name: self.name.clone(),
                length,
            });
        }
        if self.position_m.iter().any(|value| !value.is_finite()) {
            return Err(self.bad_value("position_m", "every number must be finite"));
        }
        if !self.max_force_n.is_finite() || self.max_force_n <= 0.0 {
            return Err(self.bad_value("max_force_n", "must be a finite number above zero"));
        }
        Ok(())
    }

    fn bad_value(&self, field: &'static str, reason: &'static str) -> VehicleConfigError {
        VehicleConfigError::BadNozzleValue {
            name: self.name.clone(),
            field,
            reason,
        }
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

    #[test]
    fn the_repo_vehicle_config_loads() {
        let config = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();

        assert_eq!(config.chassis.dry_mass_kg, 1800.0);
        assert_eq!(config.wheels.mount_points_m.len(), 4);
    }

    #[test]
    fn front_left_wheel_is_in_front_and_on_the_left() {
        let config = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
        let front_left = config.wheels.mount_points_m[0];

        assert!(front_left[0] > 0.0, "front means +X");
        assert!(front_left[2] < 0.0, "left means -Z");
    }

    #[test]
    fn a_negative_mass_is_rejected_with_the_field_name() {
        let broken = DEFAULT_VEHICLE_TOML.replace("dry_mass_kg = 1800.0", "dry_mass_kg = -5.0");

        let error = VehicleConfig::from_toml_str(&broken).unwrap_err();

        assert!(error.to_string().contains("chassis.dry_mass_kg"));
    }

    #[test]
    fn an_infinite_speed_limit_is_rejected() {
        let broken = DEFAULT_VEHICLE_TOML.replace("max_speed_m_s = 45.0", "max_speed_m_s = inf");

        let error = VehicleConfig::from_toml_str(&broken).unwrap_err();

        assert!(error.to_string().contains("drive.max_speed_m_s"));
    }

    #[test]
    fn an_unknown_field_is_rejected() {
        let broken = DEFAULT_VEHICLE_TOML.replace("[drive]", "[drive]\nturbo = true");

        let error = VehicleConfig::from_toml_str(&broken).unwrap_err();

        assert!(error.to_string().contains("turbo"));
    }

    #[test]
    fn nozzles_out_of_order_are_rejected() {
        let broken = DEFAULT_VEHICLE_TOML.replacen("name = \"L_FL\"", "name = \"L_FR\"", 1);

        let error = VehicleConfig::from_toml_str(&broken).unwrap_err();

        assert!(matches!(
            error,
            VehicleConfigError::WrongNozzleOrder { index: 0, .. }
        ));
    }

    #[test]
    fn a_nozzle_direction_that_is_not_unit_length_is_rejected() {
        let broken = DEFAULT_VEHICLE_TOML.replacen(
            "direction = [0.0, 1.0, 0.0]",
            "direction = [0.0, 2.0, 0.0]",
            1,
        );

        let error = VehicleConfig::from_toml_str(&broken).unwrap_err();

        assert!(error.to_string().contains("L_FL"));
    }

    #[test]
    fn a_bad_nozzle_force_names_the_nozzle() {
        let broken = DEFAULT_VEHICLE_TOML.replacen("max_force_n = 8000.0", "max_force_n = 0.0", 1);

        let error = VehicleConfig::from_toml_str(&broken).unwrap_err();

        assert!(error.to_string().contains("B_L"), "{error}");
    }

    #[test]
    fn reversed_steering_limits_are_rejected() {
        let broken = DEFAULT_VEHICLE_TOML.replace(
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
