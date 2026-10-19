//! The driving stages C, D and E (SPEC §9.5, §9.6): their settings in scenarios.toml and
//! the per-episode draws that set each one up. Stage C hops on a straight flat track,
//! sometimes over a row of parked cars; Stage D drives a procedural course; Stage E is
//! A, C or D with Chaos knocks. `episode::Episode` applies these; every draw comes from
//! the episode seed on ChaCha8 streams of their own, so a seed always gives the same start.

use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::Deserialize;

use crate::conventions::PHYSICS_HZ;
use crate::course::{BoundsSpec, CourseSpec, PieceSpec, StartSpec};
use crate::course_build::car_row_length_m;
use crate::course_gen::PieceCatalog;
use crate::hop_profile::HopProfile;
use crate::scenarios::{Stage, draw};
use crate::vehicle_config::VehicleConfig;

/// The ChaCha8 stream for Stage C draws (scenario starts use 1 for Stages A and B too,
/// but a C episode never draws an A or B start).
const HOP_STAGE_RNG_STREAM: u64 = 1;
/// The ChaCha8 stream for Stage E's base stage and knocks.
const CHAOS_STAGE_RNG_STREAM: u64 = 6;
/// The ChaCha8 stream for Stage D's target speed.
const TARGET_SPEED_RNG_STREAM: u64 = 7;
/// Course bounds reach this far below and above the floor, in m.
const BOUNDS_LOW_M: f32 = -20.0;
const BOUNDS_HIGH_M: f32 = 60.0;

/// Stage C, the hop (`[stage_c]`). Ranges are [low, high], drawn evenly.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HopStageConfig {
    /// Episodes are cut off after this long, in s.
    pub time_limit_s: f32,
    /// Starting speed, which the driver bot then holds, in m/s.
    pub speed_m_s: [f32; 2],
    /// When the Hop is pressed, in s after the start.
    pub hop_time_s: [f32; 2],
    /// Chance of a row of parked cars to hop over.
    pub car_row_probability: f32,
    /// Cars in the row, low and high inclusive (fewer if the hop could not clear them).
    pub car_row_count: [u32; 2],
    /// Height of the parked cars, in m.
    pub car_row_height_m: [f32; 2],
    /// Extra distance the car must be clear for, beyond the row and its own length, in m.
    pub spare_length_m: f32,
    /// The straight runs this far past where the car could reach in the time limit, in m.
    pub run_out_m: f32,
}

/// Stage D, the procedural course (`[stage_d]`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CourseStageConfig {
    /// Episodes are cut off after this long, in s.
    pub time_limit_s: f32,
    /// The bot's target speed is at least the course length over this share of the time
    /// limit, so a slow draw can still finish in time.
    pub time_use_fraction: f32,
}

/// Stage E, Chaos (`[stage_e]`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChaosStageConfig {
    /// Episodes are cut off after this long, in s.
    pub time_limit_s: f32,
    /// Relative chances of the base stage: A, C, D.
    pub base_stage_weights: [f32; 3],
    /// How many Chaos knocks, low and high inclusive.
    pub knock_count: [u32; 2],
    /// When each knock comes, in s after the start.
    pub knock_time_s: [f32; 2],
}

/// The stages Stage E builds on, in `base_stage_weights` order.
const CHAOS_BASE_STAGES: [Stage; 3] = [Stage::A, Stage::C, Stage::D];

/// One Stage C episode's draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HopSetup {
    /// Starting (and target) speed, in m/s.
    pub speed_m_s: f32,
    /// When the bot presses Hop when there is no car row, in s.
    pub hop_time_s: f32,
    /// The car row, if this episode has one.
    pub car_row: Option<CarRowSetup>,
}

/// A Stage C car row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarRowSetup {
    /// Where the row starts along the track, in m from the car's start.
    pub start_m: f32,
    /// How many cars.
    pub count: u32,
    /// How tall they are, in m.
    pub height_m: f32,
}

/// The draws for the Stage C episode with `seed`, for this car and its hop `profile`. The row (if
/// any) is placed so that a Hop pressed at `hop_time_s` keeps the wheels `clearance_m`
/// above the roofs for longest over its middle, and it keeps only as many cars as that
/// hop can clear (none if it cannot clear one).
pub fn draw_hop_setup(
    config: &HopStageConfig,
    clearance_m: f32,
    vehicle: &VehicleConfig,
    profile: &HopProfile,
    seed: u64,
) -> HopSetup {
    let mut rng = stream_rng(seed, HOP_STAGE_RNG_STREAM);
    let speed_m_s = draw(&mut rng, config.speed_m_s);
    let hop_time_s = draw(&mut rng, config.hop_time_s);
    let wants_row = rng.random_range(0.0..1.0) < config.car_row_probability;
    let [fewest, most] = config.car_row_count;
    let wanted_count = rng.random_range(fewest..=most);
    let height_m = draw(&mut rng, config.car_row_height_m);
    let mut car_row = None;
    let window = profile.clear_window(height_m + clearance_m);
    if let (true, Some(window)) = (wants_row, window) {
        let car_length_m = 2.0 * vehicle.chassis.half_extents_m[0];
        let clear_m = speed_m_s * (window.until_s - window.from_s);
        let mut count = wanted_count;
        while count > 0 && car_row_length_m(count) + car_length_m + config.spare_length_m > clear_m
        {
            count -= 1;
        }
        if count > 0 {
            let middle_m = speed_m_s * (hop_time_s + window.middle_s());
            let start_m = middle_m - car_row_length_m(count) / 2.0;
            car_row = Some(CarRowSetup {
                start_m,
                count,
                height_m,
            });
        }
    }
    HopSetup {
        speed_m_s,
        hop_time_s,
        car_row,
    }
}

/// The straight, flat Stage C track: long enough for `setup`'s speed over
/// `time_limit_s` (the limit of the stage running it: C's own, or E's when E builds on
/// C), `catalog.track_half_width_m` wide, with the car row (marked `hop_hint`) if any.
pub fn hop_course(
    config: &HopStageConfig,
    catalog: &PieceCatalog,
    setup: &HopSetup,
    time_limit_s: f32,
) -> CourseSpec {
    let length_m = setup.speed_m_s * time_limit_s + config.run_out_m;
    let mut pieces = Vec::new();
    if let Some(row) = setup.car_row {
        pieces.push(PieceSpec::CarRow {
            pos: [row.start_m, 0.0, 0.0],
            yaw: 0.0,
            count: row.count,
            height: row.height_m,
            hop_hint: true,
        });
    }
    straight_course("Stage C hop", catalog, length_m, pieces)
}

/// A course along +X from the origin to `end_x_m`, with the catalog's start height,
/// width and margins.
pub fn straight_course(
    name: &str,
    catalog: &PieceCatalog,
    end_x_m: f32,
    pieces: Vec<PieceSpec>,
) -> CourseSpec {
    let half_width_m = catalog.track_half_width_m;
    let margin_m = catalog.bounds_margin_m;
    CourseSpec {
        name: name.to_string(),
        start: StartSpec {
            pos: [0.0, catalog.start_height_m, 0.0],
            yaw: 0.0,
        },
        bounds: BoundsSpec {
            min: [-margin_m, BOUNDS_LOW_M, -half_width_m],
            max: [end_x_m + margin_m, BOUNDS_HIGH_M, half_width_m],
        },
        pieces,
    }
}

/// One Stage E episode's draws: the stage it builds on and the physics steps on which
/// Chaos knocks (sorted, at least two steps apart so each is a fresh button press).
#[derive(Debug, Clone, PartialEq)]
pub struct ChaosPlan {
    /// A, C or D.
    pub base_stage: Stage,
    /// Physics step numbers (from the episode start) of the knocks.
    pub knock_steps: Vec<u64>,
}

/// The draws for the Stage E episode with `seed`.
pub fn draw_chaos_plan(config: &ChaosStageConfig, seed: u64) -> ChaosPlan {
    let mut rng = stream_rng(seed, CHAOS_STAGE_RNG_STREAM);
    let base_stage = pick_weighted(&CHAOS_BASE_STAGES, &config.base_stage_weights, &mut rng);
    let [fewest, most] = config.knock_count;
    let count = rng.random_range(fewest..=most);
    let mut knock_steps = Vec::new();
    for _ in 0..count {
        let time_s = draw(&mut rng, config.knock_time_s);
        knock_steps.push((time_s * PHYSICS_HZ as f32).round() as u64);
    }
    knock_steps.sort_unstable();
    for index in 1..knock_steps.len() {
        knock_steps[index] = knock_steps[index].max(knock_steps[index - 1] + 2);
    }
    ChaosPlan {
        base_stage,
        knock_steps,
    }
}

/// The driver bot's target speed for the Stage D episode with `seed`: even in
/// `range_m_s`, but at least `at_least_m_s` (capped at the range's top), in m/s.
pub fn draw_target_speed(range_m_s: [f32; 2], at_least_m_s: f32, seed: u64) -> f32 {
    let mut rng = stream_rng(seed, TARGET_SPEED_RNG_STREAM);
    let [low, high] = range_m_s;
    let low = low.max(at_least_m_s).min(high);
    draw(&mut rng, [low, high])
}

/// One of `choices`, each as likely as its weight (weights ≥ 0, not all zero).
pub(crate) fn pick_weighted<T: Copy>(choices: &[T], weights: &[f32], rng: &mut ChaCha8Rng) -> T {
    let total: f32 = weights.iter().sum();
    let mut left = rng.random_range(0.0..1.0) * total;
    for (choice, weight) in choices.iter().zip(weights) {
        if left < *weight {
            return *choice;
        }
        left -= weight;
    }
    // Rounding can leave a sliver past the last weight; the last choice with any
    // weight takes it.
    let mut last = choices[0];
    for (choice, weight) in choices.iter().zip(weights) {
        if *weight > 0.0 {
            last = *choice;
        }
    }
    last
}

/// A generator for the episode with `seed` on ChaCha8 stream `stream`.
pub(crate) fn stream_rng(seed: u64, stream: u64) -> ChaCha8Rng {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    rng.set_stream(stream);
    rng
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_picks_follow_their_weights_and_never_pick_a_zero_weight() {
        let mut rng = stream_rng(1, 0);
        let mut counts = [0; 3];

        for _ in 0..3000 {
            let pick = pick_weighted(&[0, 1, 2], &[1.0, 0.0, 2.0], &mut rng);
            counts[pick] += 1;
        }

        assert_eq!(counts[1], 0);
        assert!((800..1200).contains(&counts[0]), "{counts:?}");
    }

    #[test]
    fn knocks_are_sorted_and_at_least_two_steps_apart() {
        let config = ChaosStageConfig {
            time_limit_s: 45.0,
            base_stage_weights: [1.0, 1.0, 1.0],
            knock_count: [3, 3],
            knock_time_s: [1.0, 1.01],
        };

        for seed in 0..50 {
            let plan = draw_chaos_plan(&config, seed);
            assert_eq!(plan.knock_steps.len(), 3);
            for pair in plan.knock_steps.windows(2) {
                assert!(pair[1] >= pair[0] + 2, "{:?}", plan.knock_steps);
            }
        }
    }
}
