//! The actuator predictor: what the thrusters will push over the next control interval
//! for a given controller command, without touching the world.
//!
//! `World::actuator_state` copies the valves, tank and Hop/Boost state; `predict` steps
//! those copies through `PHYSICS_STEPS_PER_CONTROL_STEP` physics steps with the same
//! functions and in the same order as `World::step_car`. It uses no randomness. The
//! actuator-aware allocation (nl-control) plans with it.

use rapier3d::prelude::*;

use crate::car_state::{WRENCH_SIZE, Wrench};
use crate::conventions::{NOZZLE_COUNT, PHYSICS_DT_S, PHYSICS_STEPS_PER_CONTROL_STEP};
use crate::hop_boost::{HopBoost, combine_throttles};
use crate::modes::ModeParams;
use crate::tank::AirTank;
use crate::thrusters::{Valves, safe_throttle, scaled_push_n};

/// Everything that decides what the nozzles push next, copied out of a `World`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActuatorState {
    pub(crate) valves: Valves,
    pub(crate) hop_boost: HopBoost,
    pub(crate) tank: AirTank,
    pub(crate) mode: ModeParams,
    /// The driver holds Boost (the last input); the predictor assumes they keep holding it.
    pub(crate) boost_held: bool,
    /// `boost_speed_share` at the current speed.
    pub(crate) boost_share: f32,
    /// Wheels touching now; Hop and refill use it.
    pub(crate) wheels_down: usize,
    /// `World::thrusters_locked` now.
    pub(crate) thrusters_locked: bool,
    pub(crate) valve_time_constant_s: f32,
    pub(crate) exhaust_speed_m_s: f32,
    /// Each nozzle's push at full throttle in a ×1 mode, body frame, in N.
    pub(crate) full_push_body_n: [[f32; 3]; NOZZLE_COUNT],
    /// From the true centre of mass to each nozzle, body frame, in m.
    pub(crate) lever_body_m: [[f32; 3]; NOZZLE_COUNT],
}

/// What one control interval of the thrusters is predicted to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntervalPrediction {
    /// Mean thrust force and torque over the interval, about the true centre of mass,
    /// in the body frame: [force x, y, z, torque x, y, z] in N and N·m.
    pub mean_wrench: Wrench,
    /// The mean push of the Hop/Boost throttles alone, as if the valves were already at
    /// them, at this tank gate: the driver's part of `mean_wrench`.
    pub feed_forward_wrench: Wrench,
    /// Mean valve opening of each nozzle over the interval, 0..1.
    pub mean_valve_openings: [f32; NOZZLE_COUNT],
    /// Mean Hop/Boost throttle of each nozzle over the interval, 0..1.
    pub mean_feed_forward: [f32; NOZZLE_COUNT],
    /// Largest Hop/Boost throttle of each nozzle in the interval: the controller has
    /// only `1 −` this left on that nozzle.
    pub max_feed_forward: [f32; NOZZLE_COUNT],
    /// True where feed-forward plus the controller's throttle passed 1 and was clamped.
    pub saturated: [bool; NOZZLE_COUNT],
    /// Valve openings at the end of the interval.
    pub final_valve_openings: [f32; NOZZLE_COUNT],
    /// Air the interval uses (counted even with unlimited air), in kg.
    pub air_used_kg: f32,
    /// Air left at the end, in kg.
    pub air_left_kg: f32,
    /// The tank gate at the end, 0..1 (SPEC §7.5).
    pub final_tank_gate: f32,
    /// The interlock cut the controller's throttles.
    pub thrusters_locked: bool,
}

/// Running sums over the interval's physics steps.
#[derive(Default)]
struct IntervalSums {
    wrench: Wrench,
    feed_forward_wrench: Wrench,
    valve_openings: [f32; NOZZLE_COUNT],
    feed_forward: [f32; NOZZLE_COUNT],
    max_feed_forward: [f32; NOZZLE_COUNT],
    saturated: [bool; NOZZLE_COUNT],
}

impl ActuatorState {
    /// Predicts one control interval with the controller holding `controller_throttles`
    /// (0..1, nozzle order). The driver is assumed to press nothing new and keep holding
    /// what they hold; the car's speed, wheel contact and interlock are taken as they are
    /// now for the whole interval.
    pub fn predict(&self, controller_throttles: [f32; NOZZLE_COUNT]) -> IntervalPrediction {
        let mut valves = self.valves;
        let mut hop_boost = self.hop_boost;
        let mut tank = self.tank;
        let mut sums = IntervalSums::default();
        let controller = if self.thrusters_locked {
            [0.0; NOZZLE_COUNT]
        } else {
            controller_throttles
        };
        // World::step_car, one physics step at a time.
        for _ in 0..PHYSICS_STEPS_PER_CONTROL_STEP {
            hop_boost.update(
                false,
                self.boost_held,
                self.wheels_down,
                tank.air_kg(),
                &self.mode,
                PHYSICS_DT_S,
            );
            let feed_forward = hop_boost.feed_forward_throttles(&self.mode, self.boost_share);
            let commanded = combine_throttles(feed_forward, controller);
            let multiplier = self.mode.thrust_scale * tank.thrust_gate(&self.mode);
            valves.update(commanded, PHYSICS_DT_S, self.valve_time_constant_s);
            let openings = valves.effective_throttles();
            let (wrench, total_force_n) = self.push(openings, multiplier);
            let (feed_forward_wrench, _) = self.push(feed_forward, multiplier);
            tank.use_air(
                total_force_n,
                self.exhaust_speed_m_s,
                PHYSICS_DT_S,
                &self.mode,
            );
            tank.refill(self.wheels_down, PHYSICS_DT_S, &self.mode);
            sums.add_step(
                &wrench,
                &feed_forward_wrench,
                openings,
                feed_forward,
                controller,
            );
        }
        let air_used_kg = tank.air_used_kg() - self.tank.air_used_kg();
        sums.into_prediction(
            valves,
            &tank,
            &self.mode,
            air_used_kg,
            self.thrusters_locked,
        )
    }

    /// How much of a change in the controller's throttle shows up in the interval's
    /// mean valve opening (0..1; the same for every nozzle). About a third with the
    /// default 30 ms valves: the rest arrives in later intervals.
    pub fn controller_gain(&self) -> f32 {
        let mut valves = Valves::default();
        let mut sum = 0.0;
        for _ in 0..PHYSICS_STEPS_PER_CONTROL_STEP {
            valves.update(
                [1.0; NOZZLE_COUNT],
                PHYSICS_DT_S,
                self.valve_time_constant_s,
            );
            sum += valves.effective_throttles()[0];
        }
        sum / PHYSICS_STEPS_PER_CONTROL_STEP as f32
    }

    /// The mode's thrust relative to the Realistic baseline.
    pub fn mode_thrust_scale(&self) -> f32 {
        self.mode.thrust_scale
    }

    /// True when the interlock cuts the controller's throttles now.
    pub fn thrusters_locked(&self) -> bool {
        self.thrusters_locked
    }

    /// The force and torque about the centre of mass of `throttles` (valve openings)
    /// times `multiplier`, and the summed force size the tank pays for. The same sums
    /// as `thrusters::apply_nozzle_forces`.
    fn push(&self, throttles: [f32; NOZZLE_COUNT], multiplier: f32) -> (Wrench, f32) {
        let mut wrench = [0.0; WRENCH_SIZE];
        let mut total_force_n = 0.0;
        for (nozzle, throttle) in throttles.into_iter().enumerate() {
            if throttle <= 0.0 {
                continue;
            }
            let full_push_n = Vector::from_array(self.full_push_body_n[nozzle]);
            let force_n = scaled_push_n(full_push_n, throttle, multiplier);
            total_force_n += force_n.length();
            let torque_n_m = Vector::from_array(self.lever_body_m[nozzle]).cross(force_n);
            let [fx, fy, fz] = force_n.to_array();
            let [tx, ty, tz] = torque_n_m.to_array();
            for (sum, part) in wrench.iter_mut().zip([fx, fy, fz, tx, ty, tz]) {
                *sum += part;
            }
        }
        (wrench, total_force_n)
    }
}

impl IntervalSums {
    fn add_step(
        &mut self,
        wrench: &Wrench,
        feed_forward_wrench: &Wrench,
        openings: [f32; NOZZLE_COUNT],
        feed_forward: [f32; NOZZLE_COUNT],
        controller: [f32; NOZZLE_COUNT],
    ) {
        for row in 0..WRENCH_SIZE {
            self.wrench[row] += wrench[row];
            self.feed_forward_wrench[row] += feed_forward_wrench[row];
        }
        for nozzle in 0..NOZZLE_COUNT {
            self.valve_openings[nozzle] += openings[nozzle];
            self.feed_forward[nozzle] += feed_forward[nozzle];
            self.max_feed_forward[nozzle] = self.max_feed_forward[nozzle].max(feed_forward[nozzle]);
            // combine_throttles clamps this sum to 1.
            if feed_forward[nozzle] + safe_throttle(controller[nozzle]) > 1.0 {
                self.saturated[nozzle] = true;
            }
        }
    }

    fn into_prediction(
        self,
        valves: Valves,
        tank: &AirTank,
        mode: &ModeParams,
        air_used_kg: f32,
        thrusters_locked: bool,
    ) -> IntervalPrediction {
        let steps = PHYSICS_STEPS_PER_CONTROL_STEP as f32;
        IntervalPrediction {
            mean_wrench: self.wrench.map(|sum| sum / steps),
            feed_forward_wrench: self.feed_forward_wrench.map(|sum| sum / steps),
            mean_valve_openings: self.valve_openings.map(|sum| sum / steps),
            mean_feed_forward: self.feed_forward.map(|sum| sum / steps),
            max_feed_forward: self.max_feed_forward,
            saturated: self.saturated,
            final_valve_openings: valves.effective_throttles(),
            air_used_kg,
            air_left_kg: tank.air_kg(),
            final_tank_gate: tank.thrust_gate(mode),
            thrusters_locked,
        }
    }
}
