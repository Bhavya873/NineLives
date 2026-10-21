//! Passenger g-load (SPEC §9.4): the specific force felt at the driver's seat, in
//! multiples of g, smoothed over `[passenger] smoothing_s`. 1 g sitting still, 0 in free
//! fall, high during hard braking pushes, landings and fast spins. The reward charges for
//! it above a soft limit and the game darkens the screen; it is not an observation.

use rapier3d::prelude::*;
use serde::{Deserialize, Serialize};

use crate::conventions::GRAVITY_M_S2;

/// The smoothed passenger g-load, carried from step to step with the velocities the last
/// step ended on. The acceleration is the change from those, so it includes the wheel
/// forces applied before each step as well as gravity and thrust during it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GLoad {
    smoothed_g: f32,
    /// Linear and angular velocity at the end of the last step, world frame; None right
    /// after the car was placed, so a placement is not felt as a jolt.
    last_velocity: Option<([f32; 3], [f32; 3])>,
}

impl Default for GLoad {
    /// Sitting still: 1 g, with no step measured yet.
    fn default() -> GLoad {
        GLoad {
            smoothed_g: 1.0,
            last_velocity: None,
        }
    }
}

impl GLoad {
    /// The smoothed g-load now, in g.
    pub fn g(&self) -> f32 {
        self.smoothed_g
    }

    /// Forgets the last velocities, e.g. after the car is placed or respawned.
    pub fn forget_motion(&mut self) {
        self.last_velocity = None;
    }

    /// Measures the step `body` just took (seat at `seat_body_m`, body frame) and folds
    /// it into the smoothed g-load with time constant `smoothing_s`.
    pub fn update(&mut self, body: &RigidBody, seat_body_m: [f32; 3], dt_s: f32, smoothing_s: f32) {
        if let Some(before) = self.last_velocity {
            let instant_g = seat_g(before, body, seat_body_m, dt_s);
            self.smooth(instant_g, dt_s, smoothing_s);
        }
        self.last_velocity = Some((body.linvel().to_array(), body.angvel().to_array()));
    }

    /// First-order filter: a single contact step's spike is not what a body feels.
    fn smooth(&mut self, instant_g: f32, dt_s: f32, smoothing_s: f32) {
        let blend = (dt_s / (smoothing_s + dt_s)).clamp(0.0, 1.0);
        self.smoothed_g += blend * (instant_g - self.smoothed_g);
    }
}

/// The g-load at the seat over the step just taken: the seat's acceleration
/// a = a_cm + α × r + ω × (ω × r), minus gravity, as a multiple of g. `before` holds
/// the linear and angular velocity the step started from.
fn seat_g(before: ([f32; 3], [f32; 3]), body: &RigidBody, seat_body_m: [f32; 3], dt_s: f32) -> f32 {
    let linear_accel = (body.linvel() - Vector::from_array(before.0)) / dt_s;
    let angular_velocity = body.angvel();
    let angular_accel = (angular_velocity - Vector::from_array(before.1)) / dt_s;
    let seat_m = body.rotation() * Vector::from_array(seat_body_m);
    let seat_accel = linear_accel
        + angular_accel.cross(seat_m)
        + angular_velocity.cross(angular_velocity.cross(seat_m));
    let felt = seat_accel - Vector::new(0.0, -GRAVITY_M_S2, 0.0);
    felt.length() / GRAVITY_M_S2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_moves_part_way_each_step_and_settles_on_a_steady_value() {
        let mut load = GLoad::default();

        load.smooth(3.0, 0.01, 0.09);
        let after_one = load.g();
        for _ in 0..200 {
            load.smooth(3.0, 0.01, 0.09);
        }

        assert!((after_one - 1.2).abs() < 1e-6);
        assert!((load.g() - 3.0).abs() < 1e-3);
    }
}
