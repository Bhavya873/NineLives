//! Fixed project conventions from SPEC §4.
//!
//! These values are requirements, not tuning knobs, so they live in code
//! instead of `configs/*.toml`. Every other module uses these names and
//! orders, so the Python trainer and the browser agree on them too.

use rapier3d::math::Vector;

/// Length of one physics step, in seconds. SPEC §4: 120 Hz.
pub const PHYSICS_DT_S: f32 = 1.0 / 120.0;

/// The controller runs once every this many physics steps (60 Hz). SPEC §4.
pub const PHYSICS_STEPS_PER_CONTROL_STEP: u32 = 2;

/// Length of one control step, in seconds (1/60 s).
pub const CONTROL_DT_S: f32 = PHYSICS_DT_S * PHYSICS_STEPS_PER_CONTROL_STEP as f32;

/// Gravity acceleration, in m/s². The world's +Y axis points up.
pub const GRAVITY_M_S2: f32 = 9.81;

/// Number of cold-gas nozzles on the car.
pub const NOZZLE_COUNT: usize = 10;

/// Number of wheels on the car.
pub const WHEEL_COUNT: usize = 4;

/// Nozzle names in their fixed index order (SPEC §4).
/// L = lift, S = side, B = boost; FL = front-left, RR = rear-right, and so on.
pub const NOZZLE_NAMES: [&str; NOZZLE_COUNT] = [
    "L_FL", "L_FR", "L_RL", "L_RR", "S_FL", "S_RL", "S_FR", "S_RR", "B_L", "B_R",
];

/// Wheel names in their fixed index order (SPEC §4).
pub const WHEEL_NAMES: [&str; WHEEL_COUNT] = ["FL", "FR", "RL", "RR"];

/// The gravity vector in the world frame, in m/s².
pub fn gravity_vector() -> Vector {
    Vector::new(0.0, -GRAVITY_M_S2, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gravity_points_down_the_world_y_axis() {
        let gravity = gravity_vector();

        assert_eq!(gravity.x, 0.0);
        assert!(gravity.y < 0.0);
        assert_eq!(gravity.z, 0.0);
    }

    #[test]
    fn control_runs_at_60_hz() {
        let control_hz = 1.0 / CONTROL_DT_S;

        assert!((control_hz - 60.0).abs() < 1e-3);
    }

    #[test]
    fn nozzle_names_are_unique() {
        for (i, first) in NOZZLE_NAMES.iter().enumerate() {
            for second in &NOZZLE_NAMES[i + 1..] {
                assert_ne!(first, second);
            }
        }
    }

    #[test]
    fn nozzle_order_matches_the_spec() {
        // SPEC §4 fixes these indices; actions, observations and the UI all rely on them.
        assert_eq!(NOZZLE_NAMES[0], "L_FL");
        assert_eq!(NOZZLE_NAMES[4], "S_FL");
        assert_eq!(NOZZLE_NAMES[8], "B_L");
        assert_eq!(NOZZLE_NAMES[9], "B_R");
    }
}
