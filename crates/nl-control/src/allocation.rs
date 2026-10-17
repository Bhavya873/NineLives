//! Thruster allocation (SPEC §8): which throttles (0..1 each) best produce a wanted
//! force and torque on the car. Thrust is one-sided, so this is a bounded least-squares
//! problem: minimise |W(B·u − w)|² + λ|u|² with 0 ≤ u ≤ 1, solved by projected gradient
//! descent, warm-started from the previous answer.

use nl_sim::conventions::NOZZLE_COUNT;

/// Rows of the wrench: force x, y, z then torque x, y, z (body frame).
pub const WRENCH_SIZE: usize = 6;

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
    /// 1 / L, the gradient step: L = 2(|WB|²_F + λ) bounds the gradient's Lipschitz constant.
    step: f32,
}

impl Allocator {
    /// Builds the problem from each nozzle's full-throttle wrench (the columns of `B`).
    pub fn new(
        nozzle_wrenches: &[[f32; WRENCH_SIZE]; NOZZLE_COUNT],
        config: &AllocationConfig,
    ) -> Allocator {
        let mut row_weights = [0.0; WRENCH_SIZE];
        for (row, weight) in row_weights.iter_mut().enumerate() {
            let strongest = strongest_in_row(nozzle_wrenches, row);
            let base = if row < 3 {
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
        let mut frobenius_sq = 0.0;
        for (row, weight) in row_weights.iter().enumerate() {
            for nozzle in 0..NOZZLE_COUNT {
                let value = weight * nozzle_wrenches[nozzle][row];
                weighted_b[row][nozzle] = value;
                frobenius_sq += value * value;
            }
        }
        Allocator {
            weighted_b,
            row_weights,
            regularisation: config.regularisation,
            iterations: config.iterations,
            step: 1.0 / (2.0 * (frobenius_sq + config.regularisation)),
        }
    }

    /// Improves `throttles` (each 0..1, updated in place) toward producing `wrench`
    /// when every nozzle's push is multiplied by `thrust_scale`.
    pub fn solve(
        &self,
        wrench: [f32; WRENCH_SIZE],
        thrust_scale: f32,
        throttles: &mut [f32; NOZZLE_COUNT],
    ) {
        // Weaker thrust means a bigger throttle for the same push: aim at wrench / scale.
        let scale = if thrust_scale > 0.0 {
            thrust_scale
        } else {
            1.0
        };
        let mut target = [0.0; WRENCH_SIZE];
        for row in 0..WRENCH_SIZE {
            target[row] = self.row_weights[row] * wrench[row] / scale;
        }
        for _ in 0..self.iterations {
            let residual = self.residual(throttles, &target);
            for (nozzle, throttle) in throttles.iter_mut().enumerate() {
                let gradient = self.gradient(&residual, *throttle, nozzle);
                *throttle = (*throttle - self.step * gradient).clamp(0.0, 1.0);
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

/// The largest |value| any nozzle has in one wrench row.
fn strongest_in_row(nozzle_wrenches: &[[f32; WRENCH_SIZE]; NOZZLE_COUNT], row: usize) -> f32 {
    let mut strongest: f32 = 0.0;
    for wrench in nozzle_wrenches {
        strongest = strongest.max(wrench[row].abs());
    }
    strongest
}
