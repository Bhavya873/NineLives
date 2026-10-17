//! The PD baseline controller (SPEC §8): turn the car upright with attitude torque, brake
//! a fast fall just before landing, and roll back onto the wheels when lying on the
//! ground. It decides a wanted force and torque, then the allocation picks throttles.

use nl_sim::car_state::{CarModel, CarState};
use nl_sim::conventions::{GRAVITY_M_S2, NOZZLE_COUNT};

use crate::allocation::{Allocator, WRENCH_SIZE};
use crate::config::{ControllersConfig, PdConfig};
use crate::{ControlCtx, Controller};

/// Wheels on the ground at which the car counts as driving, so PD stays off (saves air).
const DRIVING_WHEELS: usize = 3;
/// Nearly upside down: the tilt error's direction is undefined, so pick a roll.
const NEARLY_INVERTED_UP_Y: f32 = -0.95;

/// The PD controller with its allocation, for one car.
#[derive(Debug, Clone, PartialEq)]
pub struct Pd {
    config: PdConfig,
    mass_kg: f32,
    inertia_kg_m2: [f32; 3],
    allocator: Allocator,
    /// Last throttles, the warm start for the next allocation.
    throttles: [f32; NOZZLE_COUNT],
}

impl Pd {
    /// A PD controller for the car described by `model`.
    pub fn new(model: &CarModel, config: &ControllersConfig) -> Pd {
        Pd {
            config: config.pd,
            mass_kg: model.mass_kg,
            inertia_kg_m2: model.principal_inertia_kg_m2,
            allocator: Allocator::new(&model.nozzle_wrenches, &config.allocation),
            throttles: [0.0; NOZZLE_COUNT],
        }
    }

    /// The force and torque PD wants now, in the body frame:
    /// [force x, y, z, torque x, y, z]. All zero while the car is driving.
    pub fn wanted_wrench(&self, state: &CarState) -> [f32; WRENCH_SIZE] {
        if state.wheels_down >= DRIVING_WHEELS {
            return [0.0; WRENCH_SIZE];
        }
        let mut torque = self.attitude_torque(state);
        let lying_on_ground = state.height_m < self.config.on_ground_height_m
            && state.up_alignment < self.config.upright_alignment;
        if lying_on_ground {
            // SPEC §8: roll torque only, to flip back onto the wheels.
            torque[1] = 0.0;
            torque[2] = 0.0;
        }
        let force = self.braking_force_body(state);
        [
            force[0], force[1], force[2], torque[0], torque[1], torque[2],
        ]
    }

    /// SPEC §8: up_des = −gravity_dir_body; error e = (0, 1, 0) × up_des;
    /// τ = I · (kp·e − kd·ω), per body axis.
    fn attitude_torque(&self, state: &CarState) -> [f32; 3] {
        let [gx, gy, gz] = state.gravity_dir_body;
        let up_wanted = [-gx, -gy, -gz];
        // (0, 1, 0) × (x, y, z) = (z, 0, −x).
        let mut error = [up_wanted[2], 0.0, -up_wanted[0]];
        if up_wanted[1] < NEARLY_INVERTED_UP_Y && error[0].abs() < 0.1 && error[2].abs() < 0.1 {
            // Upside down, the cross product vanishes; roll over the shorter way.
            error = [if up_wanted[2] >= 0.0 { 1.0 } else { -1.0 }, 0.0, 0.0];
        }
        let spin = state.angular_velocity_body_rad_s;
        let mut torque = [0.0; 3];
        for axis in 0..3 {
            let wanted = self.config.kp * error[axis] - self.config.kd * spin[axis];
            torque[axis] = self.inertia_kg_m2[axis] * wanted;
        }
        torque
    }

    /// SPEC §8: when airborne, roughly upright, low and falling fast, push up with
    /// F = m·(g + kv·(v_down − target)); otherwise nothing, to save air. Returned in
    /// the body frame (the push is straight up in the world).
    fn braking_force_body(&self, state: &CarState) -> [f32; 3] {
        let falling_m_s = -state.linear_velocity_world_m_s[1];
        let brake = state.wheels_down == 0
            && state.up_alignment > self.config.upright_alignment
            && state.height_m < self.config.brake_height_m
            && falling_m_s > self.config.brake_speed_m_s;
        if !brake {
            return [0.0; 3];
        }
        let push_n = self.mass_kg
            * (GRAVITY_M_S2 + self.config.kv * (falling_m_s - self.config.target_descent_m_s));
        // World up in the body frame is −gravity_dir_body.
        let [gx, gy, gz] = state.gravity_dir_body;
        [-gx * push_n, -gy * push_n, -gz * push_n]
    }
}

impl Controller for Pd {
    fn reset(&mut self, _seed: u64) {
        self.throttles = [0.0; NOZZLE_COUNT];
    }

    fn act(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT] {
        let wrench = self.wanted_wrench(&ctx.state);
        if wrench == [0.0; WRENCH_SIZE] {
            self.throttles = [0.0; NOZZLE_COUNT];
        } else {
            self.allocator
                .solve(wrench, ctx.state.thrust_scale, &mut self.throttles);
        }
        self.throttles
    }
}
