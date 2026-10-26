//! Thruster allocation (SPEC §8): which throttles (0..1 each) best produce a wanted
//! force and torque on the car. Thrust is one-sided, so this is a bounded least-squares
//! problem: minimise |W(B·u − w)|² + λ|u|² with 0 ≤ u ≤ 1, solved by projected gradient
//! descent, warm-started from the previous answer.
//!
//! The actuator-aware model (`model = "actuator_aware"`) solves the same problem for the
//! push the valves will really make over the next control interval: they lag, the Hop and
//! Boost throttles sit underneath, and only the headroom above those is the controller's.

use nl_sim::actuator_model::IntervalPrediction;
use nl_sim::car_state::{FORCE_ROWS, WRENCH_SIZE, Wrench};
use nl_sim::conventions::NOZZLE_COUNT;

/// Which allocation PD uses (`[allocation] model`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationModel {
    /// Plan as if `B·u` happened at once (the original allocation).
    #[default]
    Legacy,
    /// Plan with the actuator predictor (`nl_sim::actuator_model`).
    ActuatorAware,
}

/// How the allocation trades accuracy against effort (`[allocation]` in controllers.toml).
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllocationConfig {
    /// Weight on the force rows, after scaling each row to 1 at its strongest nozzle.
    pub force_weight: f32,
    /// Weight on the torque rows; higher than force, because staying upright matters most.
    pub torque_weight: f32,
    /// λ: a small cost on every throttle, so unneeded nozzles stay shut.
    pub regularisation: f32,
    /// Projected-gradient steps per control step.
    pub iterations: u32,
    /// Which allocation to use; `legacy` when left out.
    #[serde(default)]
    pub model: AllocationModel,
}

/// What the actuator predictor says about the next control interval, as the actuator-aware
/// solve needs it (all per nozzle, in nozzle order, 0..1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActuatorPlan {
    /// Mean valve opening over the interval if the controller asked for nothing.
    pub valve_openings_with_no_command: [f32; NOZZLE_COUNT],
    /// Mean Hop/Boost throttle over the interval.
    pub mean_feed_forward: [f32; NOZZLE_COUNT],
    /// Largest Hop/Boost throttle in the interval.
    pub max_feed_forward: [f32; NOZZLE_COUNT],
    /// Share of a throttle change that shows in the interval's mean opening.
    pub controller_gain: f32,
}

impl ActuatorPlan {
    /// The plan from a prediction with no controller command (`ActuatorState::predict`
    /// of all zeros) and `ActuatorState::controller_gain`.
    pub fn from_idle_prediction(idle: &IntervalPrediction, controller_gain: f32) -> ActuatorPlan {
        ActuatorPlan {
            valve_openings_with_no_command: idle.mean_valve_openings,
            mean_feed_forward: idle.mean_feed_forward,
            max_feed_forward: idle.max_feed_forward,
            controller_gain,
        }
    }
}

/// What one actuator-aware allocation asked for and what the predictor says it gets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AllocationDiagnostics {
    /// The wrench PD asked for, body frame, in N and N·m.
    pub requested_wrench: Wrench,
    /// The controller's predicted share of the push over the interval: the predicted
    /// mean wrench minus the Hop/Boost part, about the true centre of mass.
    pub predicted_wrench: Wrench,
    /// `predicted_wrench − requested_wrench`.
    pub residual_wrench: Wrench,
    /// Nozzles whose Hop/Boost plus controller throttle was clamped at 1.
    pub saturated: [bool; NOZZLE_COUNT],
    /// Air the interval is predicted to use, in kg.
    pub air_used_kg: f32,
    /// The tank gate at the end of the interval, 0..1.
    pub tank_gate: f32,
    /// The interlock cut the controller's throttles, so nothing was solved.
    pub thrusters_locked: bool,
    /// Projected-gradient steps taken.
    pub iterations: u32,
}

impl AllocationDiagnostics {
    /// The report for `requested` given the predictor's view of the chosen throttles.
    pub fn new(
        requested: Wrench,
        prediction: &IntervalPrediction,
        iterations: u32,
    ) -> AllocationDiagnostics {
        let mut predicted_wrench = [0.0; WRENCH_SIZE];
        let mut residual_wrench = [0.0; WRENCH_SIZE];
        for row in 0..WRENCH_SIZE {
            predicted_wrench[row] =
                prediction.mean_wrench[row] - prediction.feed_forward_wrench[row];
            residual_wrench[row] = predicted_wrench[row] - requested[row];
        }
        AllocationDiagnostics {
            requested_wrench: requested,
            predicted_wrench,
            residual_wrench,
            saturated: prediction.saturated,
            air_used_kg: prediction.air_used_kg,
            tank_gate: prediction.final_tank_gate,
            thrusters_locked: prediction.thrusters_locked,
            iterations,
        }
    }
}

/// The allocation problem for one car: `B` with its rows already weighted.
#[derive(Debug, Clone, PartialEq)]
pub struct Allocator {
    /// W·B: row `r`, column `nozzle`.
    weighted_b: [[f32; NOZZLE_COUNT]; WRENCH_SIZE],
    /// W, the weight of each row.
    row_weights: [f32; WRENCH_SIZE],
    regularisation: f32,
    iterations: u32,
    /// 1 / L, the gradient step, with L bounding the gradient's Lipschitz constant.
    step: f32,
}

impl Allocator {
    /// Builds the problem from each nozzle's full-throttle wrench (the columns of `B`).
    pub fn new(nozzle_wrenches: &[Wrench; NOZZLE_COUNT], config: &AllocationConfig) -> Allocator {
        let mut row_weights = [0.0; WRENCH_SIZE];
        for (row, weight) in row_weights.iter_mut().enumerate() {
            let strongest = strongest_in_row(nozzle_wrenches, row);
            let base = if row < FORCE_ROWS {
                config.force_weight
            } else {
                config.torque_weight
            };
            *weight = if strongest > 0.0 {
                base / strongest
            } else {
                0.0
            };
        }
        let mut weighted_b = [[0.0; NOZZLE_COUNT]; WRENCH_SIZE];
        for (row, weight) in row_weights.iter().enumerate() {
            for nozzle in 0..NOZZLE_COUNT {
                let value = weight * nozzle_wrenches[nozzle][row];
                weighted_b[row][nozzle] = value;
            }
        }
        Allocator {
            weighted_b,
            row_weights,
            regularisation: config.regularisation,
            iterations: config.iterations,
            step: 1.0 / (2.0 * (largest_eigenvalue_bound(&weighted_b) + config.regularisation)),
        }
    }

    /// Improves `throttles` (each 0..1, updated in place) toward producing `wrench`
    /// when every nozzle's push is multiplied by `thrust_scale`.
    pub fn solve(&self, wrench: Wrench, thrust_scale: f32, throttles: &mut [f32; NOZZLE_COUNT]) {
        // Weaker thrust means a bigger throttle for the same push: aim at wrench / scale.
        let scale = positive_or_one(thrust_scale);
        let mut target = [0.0; WRENCH_SIZE];
        for row in 0..WRENCH_SIZE {
            target[row] = self.row_weights[row] * wrench[row] / scale;
        }
        self.descend(&target, &[1.0; NOZZLE_COUNT], throttles);
    }

    /// Like `solve`, but for the push `plan` says the valves will make over the next
    /// control interval. The interval's mean opening is the no-command opening plus
    /// `controller_gain × u`, and the controller is asked for `wrench` on top of the
    /// Hop/Boost throttles (counted once, at their mean), so it aims at
    /// `B·u = (wrench / scale − B·(no-command opening − feed-forward)) / gain`, with each
    /// nozzle held to the headroom `1 − max feed-forward`. Like `solve`, it plans with the
    /// mode's scale and not the tank gate.
    pub fn solve_actuator_aware(
        &self,
        wrench: Wrench,
        thrust_scale: f32,
        plan: &ActuatorPlan,
        throttles: &mut [f32; NOZZLE_COUNT],
    ) {
        let scale = positive_or_one(thrust_scale);
        let gain = positive_or_one(plan.controller_gain);
        let mut target = [0.0; WRENCH_SIZE];
        for row in 0..WRENCH_SIZE {
            // What the valves already give beyond the Hop/Boost throttles, weighted.
            let mut carried = 0.0;
            for nozzle in 0..NOZZLE_COUNT {
                let beyond_feed_forward =
                    plan.valve_openings_with_no_command[nozzle] - plan.mean_feed_forward[nozzle];
                carried += self.weighted_b[row][nozzle] * beyond_feed_forward;
            }
            target[row] = (self.row_weights[row] * wrench[row] / scale - carried) / gain;
        }
        let mut headroom = [1.0; NOZZLE_COUNT];
        for (room, feed_forward) in headroom.iter_mut().zip(plan.max_feed_forward) {
            *room = (1.0 - feed_forward).max(0.0);
        }
        self.descend(&target, &headroom, throttles);
    }

    /// Projected-gradient steps per solve.
    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    /// Projected gradient toward the weighted `target`, keeping each throttle in
    /// `0..=upper[nozzle]`.
    fn descend(
        &self,
        target: &[f32; WRENCH_SIZE],
        upper: &[f32; NOZZLE_COUNT],
        throttles: &mut [f32; NOZZLE_COUNT],
    ) {
        for _ in 0..self.iterations {
            let residual = self.residual(throttles, target);
            for (nozzle, throttle) in throttles.iter_mut().enumerate() {
                let gradient = self.gradient(&residual, *throttle, nozzle);
                *throttle = (*throttle - self.step * gradient).clamp(0.0, upper[nozzle]);
            }
        }
    }

    /// W·B·u − W·w for the current throttles.
    fn residual(
        &self,
        throttles: &[f32; NOZZLE_COUNT],
        target: &[f32; WRENCH_SIZE],
    ) -> [f32; WRENCH_SIZE] {
        let mut residual = [0.0; WRENCH_SIZE];
        for (row, residual_row) in residual.iter_mut().enumerate() {
            let mut produced = 0.0;
            for (weight, throttle) in self.weighted_b[row].iter().zip(throttles) {
                produced += weight * throttle;
            }
            *residual_row = produced - target[row];
        }
        residual
    }

    /// d/du of the cost for one nozzle: 2(WB)ᵀ·residual + 2λu.
    fn gradient(&self, residual: &[f32; WRENCH_SIZE], throttle: f32, nozzle: usize) -> f32 {
        let mut gradient = 2.0 * self.regularisation * throttle;
        for (row, residual_row) in residual.iter().enumerate() {
            gradient += 2.0 * self.weighted_b[row][nozzle] * residual_row;
        }
        gradient
    }
}

/// An upper bound on the largest eigenvalue of (WB)(WB)ᵀ: its largest absolute row sum
/// (Gershgorin). Much tighter than |WB|²_F for this car, so 30 steps get close to the
/// answer even from all valves shut.
fn largest_eigenvalue_bound(weighted_b: &[[f32; NOZZLE_COUNT]; WRENCH_SIZE]) -> f32 {
    let mut largest = 0.0_f32;
    for row in weighted_b {
        let mut row_sum = 0.0;
        for other_row in weighted_b {
            let mut dot = 0.0;
            for (a, b) in row.iter().zip(other_row) {
                dot += a * b;
            }
            row_sum += f32::abs(dot);
        }
        largest = largest.max(row_sum);
    }
    largest
}

/// `value` if it is positive, else 1 (a scale or gain of 0 would divide by zero).
fn positive_or_one(value: f32) -> f32 {
    if value > 0.0 { value } else { 1.0 }
}

/// The largest |value| any nozzle has in one wrench row.
fn strongest_in_row(nozzle_wrenches: &[Wrench; NOZZLE_COUNT], row: usize) -> f32 {
    let mut strongest: f32 = 0.0;
    for wrench in nozzle_wrenches {
        strongest = strongest.max(wrench[row].abs());
    }
    strongest
}
