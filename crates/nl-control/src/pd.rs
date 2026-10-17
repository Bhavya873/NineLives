//! The PD baseline controller (SPEC §8): turn the car upright with attitude torque, brake
//! a fast fall just before landing, and roll back onto the wheels when lying on the
//! ground. It decides a wanted force and torque, then the allocation picks throttles.

use nl_sim::car_state::{CarModel, CarState, WRENCH_SIZE, Wrench};
use nl_sim::conventions::{GRAVITY_M_S2, NOZZLE_COUNT};

use crate::allocation::Allocator;
use crate::config::{ControllersConfig, PdConfig};
use crate::{ControlCtx, Controller};

/// Below this size the tilt error has no reliable direction (the car is almost exactly
/// upside down), so PD picks a roll instead.
const MIN_TILT_ERROR: f32 = 0.1;

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
    /// [force x, y, z, torque x, y, z]. All zero while the car drives on its wheels,
    /// to save air.
    pub fn wanted_wrench(&self, state: &CarState) -> Wrench {
        if state.on_wheels() {
            return [0.0; WRENCH_SIZE];
        }
        let mut torque = self.attitude_torque(state);
        // Low, tipped over and nearly still: lying on the ground, not falling past it.
        let lying_on_ground = state.height_m < self.config.on_ground_height_m
            && state.up_alignment() < self.config.upright_alignment
            && state.falling_speed_m_s().abs() < self.config.lying_max_fall_speed_m_s;
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
        let upside_down = up_wanted[1] < 0.0;
        if upside_down && error[0].abs() < MIN_TILT_ERROR && error[2].abs() < MIN_TILT_ERROR {
            // The cross product vanishes; roll (either way works) at the threshold size,
            // so the torque does not jump as the error crosses it.
            let roll_sign = if up_wanted[2] >= 0.0 { 1.0 } else { -1.0 };
            error = [roll_sign * MIN_TILT_ERROR, 0.0, 0.0];
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
    /// F = m·(g + kv·(v_down − target)) along the body's +Y (w = [0, F_y, 0, …]), which
    /// only the lift nozzles provide; otherwise nothing, to save air.
    fn braking_force_body(&self, state: &CarState) -> [f32; 3] {
        let falling_m_s = state.falling_speed_m_s();
        let brake = state.wheels_down() == 0
            && state.up_alignment() > self.config.upright_alignment
            && state.height_m < self.config.brake_height_m
            && falling_m_s > self.config.brake_speed_m_s;
        if !brake {
            return [0.0; 3];
        }
        let push_n = self.mass_kg
            * (GRAVITY_M_S2 + self.config.kv * (falling_m_s - self.config.target_descent_m_s));
        [0.0, push_n, 0.0]
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
            // Plan with the mode's scale only: dividing by the tank gate would drive every
            // nozzle to full throttle as the tank runs dry (SPEC §7.5 fades it instead).
            self.allocator
                .solve(wrench, ctx.state.mode_thrust_scale, &mut self.throttles);
        }
        self.throttles
    }
}
