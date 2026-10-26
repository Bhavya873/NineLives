//! The 10 cold-gas thrusters (SPEC §7.4).
//!
//! Each nozzle has a valve. The commanded throttle (0..1) does not open the valve
//! instantly: a first-order lag smooths it (`Valves`). The effective throttle then
//! pushes the car with `throttle × max force × thrust scale` along the nozzle's
//! direction, applied at the nozzle's position so it also turns the car.

use rapier3d::prelude::*;
use serde::{Deserialize, Serialize};

use crate::conventions::NOZZLE_COUNT;
use crate::vehicle_config::{NozzleConfig, ThrusterConfig};

/// The valves' current openings: the "effective throttles" (SPEC §7.7).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Valves {
    effective_throttles: [f32; NOZZLE_COUNT],
}

impl Valves {
    /// Moves every valve one physics step toward its commanded throttle (SPEC §7.4):
    /// `u_eff += (u_cmd - u_eff) · (1 - exp(-dt / τ))`.
    pub fn update(
        &mut self,
        commanded_throttles: [f32; NOZZLE_COUNT],
        dt_s: f32,
        time_constant_s: f32,
    ) {
        let step_fraction = valve_step_fraction(dt_s, time_constant_s);
        for (effective, commanded) in self.effective_throttles.iter_mut().zip(commanded_throttles) {
            let target = safe_throttle(commanded);
            *effective += (target - *effective) * step_fraction;
        }
    }

    /// Closes every valve at once, e.g. when the car is moved to start a new run.
    pub fn close_all(&mut self) {
        self.effective_throttles = [0.0; NOZZLE_COUNT];
    }

    /// How open each valve is now, 0..1, in nozzle order.
    pub fn effective_throttles(&self) -> [f32; NOZZLE_COUNT] {
        self.effective_throttles
    }
}

/// Clamps a commanded throttle to 0..1. A NaN (for example from a broken policy)
/// counts as 0, so it can never get stuck inside the valve state.
pub(crate) fn safe_throttle(commanded: f32) -> f32 {
    if commanded.is_nan() {
        return 0.0;
    }
    commanded.clamp(0.0, 1.0)
}

/// The fraction of the remaining gap a valve closes in one step of `dt_s`.
pub fn valve_step_fraction(dt_s: f32, time_constant_s: f32) -> f32 {
    // libm keeps this bit-identical on every platform (CLAUDE.md rule 2).
    1.0 - libm::expf(-dt_s / time_constant_s)
}

/// Adds every nozzle's push to the chassis for this step and returns the total push,
/// which the air tank pays for. Call after `reset_forces`, since Rapier keeps added
/// forces until they are reset.
///
/// `thrust_scale` multiplies every nozzle's maximum force (mode scale × tank gate).
pub fn apply_nozzle_forces(
    body: &mut RigidBody,
    thrusters: &ThrusterConfig,
    effective_throttles: [f32; NOZZLE_COUNT],
    thrust_scale: f32,
) -> f32 {
    let pose = *body.position();
    let mut total_force_n = 0.0;
    for (nozzle, throttle) in thrusters.nozzles.iter().zip(effective_throttles) {
        if throttle <= 0.0 {
            continue;
        }
        let force_body_n =
            scaled_push_n(full_throttle_force_body_n(nozzle), throttle, thrust_scale);
        total_force_n += force_body_n.length();
        // Rotate the push and its point from the body frame into the world frame.
        let force_world_n = pose.rotation * force_body_n;
        let point_world_m = pose * Vector::from_array(nozzle.position_m);
        body.add_force_at_point(force_world_n, point_world_m, true);
    }
    total_force_n
}

/// The force and torque one nozzle puts on the car at full throttle in Realistic
/// mode, in the body frame. Torque is `r × F` about the centre of mass.
/// Used by the sign tests and, in M4, by the PD controller's allocation.
pub fn nozzle_force_and_torque_at_full_throttle(nozzle: &NozzleConfig) -> (Vector, Vector) {
    let force_n = full_throttle_force_body_n(nozzle);
    let torque_n_m = Vector::from_array(nozzle.position_m).cross(force_n);
    (force_n, torque_n_m)
}

/// A nozzle's push at `throttle` (valve opening) with every push multiplied by
/// `thrust_scale`, from its full-throttle push. The world and the actuator predictor both
/// use it, so they compute the same bits.
pub(crate) fn scaled_push_n(full_push_n: Vector, throttle: f32, thrust_scale: f32) -> Vector {
    full_push_n * (throttle * thrust_scale)
}

/// One nozzle's push at full throttle in Realistic mode, in the body frame.
pub(crate) fn full_throttle_force_body_n(nozzle: &NozzleConfig) -> Vector {
    Vector::from_array(nozzle.direction) * nozzle.max_force_n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conventions::PHYSICS_DT_S;

    #[test]
    fn a_valve_reaches_63_percent_after_one_time_constant() {
        // SPEC §12: the step response reaches 63% at τ ± 5%. One 120 Hz step (8.3 ms)
        // is coarser than 5% of τ (1.5 ms), so step the same valve code with a fine
        // 0.1 ms step and record when it crosses 63%.
        let time_constant_s = 0.03;
        let fine_dt_s = 0.0001;
        let mut valves = Valves::default();
        let mut commanded = [0.0; NOZZLE_COUNT];
        commanded[0] = 1.0;

        let mut elapsed_s = 0.0;
        while valves.effective_throttles()[0] < 0.632 {
            valves.update(commanded, fine_dt_s, time_constant_s);
            elapsed_s += fine_dt_s;
        }

        let error_fraction = (elapsed_s - time_constant_s).abs() / time_constant_s;
        assert!(error_fraction < 0.05, "reached 63% after {elapsed_s} s");
    }

    #[test]
    fn a_nan_command_closes_the_valve_instead_of_breaking_it() {
        let mut valves = Valves::default();
        let mut commanded = [0.0; NOZZLE_COUNT];
        commanded[2] = f32::NAN;

        valves.update(commanded, PHYSICS_DT_S, 0.03);

        assert_eq!(valves.effective_throttles()[2], 0.0);
    }

    #[test]
    fn commands_outside_0_to_1_are_clamped() {
        let mut valves = Valves::default();
        let mut commanded = [0.0; NOZZLE_COUNT];
        commanded[3] = 5.0;
        commanded[4] = -2.0;

        for _ in 0..1000 {
            valves.update(commanded, PHYSICS_DT_S, 0.03);
        }

        let effective = valves.effective_throttles();
        assert!((effective[3] - 1.0).abs() < 1e-4);
        assert_eq!(effective[4], 0.0);
    }
}
