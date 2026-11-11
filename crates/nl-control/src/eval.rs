//! The Rust eval runner (SPEC §10, M4): run a controller through many seeded episodes of
//! one stage and summarise how it did. No Python needed; the M5 Python eval steps the
//! same `nl_sim::episode::Episode`, so the numbers can be compared.

use nl_sim::conventions::{INVERTED_TILT_DEG, NOZZLE_COUNT, PHYSICS_HZ};
use nl_sim::course::CourseSpec;
use nl_sim::episode::{Episode, EpisodeResult, EpisodeStatus};
use nl_sim::input::InputFrame;
use nl_sim::randomization::EpisodeVariation;
use nl_sim::scenarios::{ScenariosConfig, Stage};
use nl_sim::world::{PlaceCarError, World};
use serde::Serialize;

use crate::{ControlCtx, Controller};

/// The whole run, summarised (SPEC §10's metrics). Rates are fractions, 0..1.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvalSummary {
    /// How many episodes ran.
    pub episodes: usize,
    /// Episodes that succeeded (four wheels down for the hold time, no crash).
    pub success_rate: f32,
    /// Episodes with at least one save.
    pub save_rate: f32,
    /// Episodes that ended in a crash.
    pub crash_rate: f32,
    /// Episodes in which the car was tipped past `INVERTED_TILT_DEG` at any moment,
    /// including a start that was already upside down (Stage A drops). Compare it
    /// between controllers on the same stage and seeds, not between stages.
    pub inversion_rate: f32,
    /// Air used during saved upsets, per save (SPEC §10), in kg; None if nothing was saved.
    pub air_per_save_kg: Option<f32>,
    /// Median spin size at first touchdown, in rad/s.
    pub touchdown_spin_median_rad_s: Option<f32>,
    /// 95th-percentile spin size at first touchdown, in rad/s.
    pub touchdown_spin_p95_rad_s: Option<f32>,
    /// Median fall speed at first touchdown, in m/s.
    pub touchdown_fall_median_m_s: Option<f32>,
    /// 95th-percentile fall speed at first touchdown, in m/s.
    pub touchdown_fall_p95_m_s: Option<f32>,
    /// Episodes with at least one hard landing (PLAN §3.3).
    pub hard_landing_rate: f32,
    /// Median heading error at the latest save's touchdown, in degrees.
    pub landing_heading_error_median_deg: Option<f32>,
    /// Median speed kept after the latest save's touchdown (fast take-offs only).
    pub speed_kept_median: Option<f32>,
    /// Median settle time, in s, over episodes that ended settled.
    pub settle_time_median_s: Option<f32>,
    /// Median air left at the end of successful episodes, in kg.
    pub air_left_at_success_median_kg: Option<f32>,
}

/// Runs one episode of `stage` from `seed` with `controller`, which acts at 60 Hz.
/// `variation` is the episode's car and world: `EpisodeVariation::none` for plain eval,
/// or a `randomization::sample_variation` for eval with domain randomization.
pub fn run_episode(
    variation: &EpisodeVariation,
    scenarios: &ScenariosConfig,
    stage: Stage,
    controller: &mut dyn Controller,
    seed: u64,
) -> Result<EpisodeResult, PlaceCarError> {
    let episode = Episode::start(variation, scenarios, stage, seed)?;
    Ok(fly_episode(episode, controller, seed))
}

/// Like `run_episode`, but on `course` (e.g. the game's playground) instead of flat
/// ground, through `Episode::start_on_course`.
pub fn run_episode_on_course(
    variation: &EpisodeVariation,
    scenarios: &ScenariosConfig,
    stage: Stage,
    course: &CourseSpec,
    controller: &mut dyn Controller,
    seed: u64,
) -> Result<EpisodeResult, PlaceCarError> {
    let episode = Episode::start_on_course(variation, scenarios, stage, course, seed)?;
    Ok(fly_episode(episode, controller, seed))
}

/// Drives `world`'s car for `seconds` holding `input`, with `controller` acting at 60 Hz
/// (no episode rules), and returns the air used, in kg. It measures firing the driver
/// didn't need, e.g. over `courses/bumps-strip.json` (owner, round 7: thrusters must not
/// fire whenever a wheel lifts).
pub fn air_used_driving(
    world: &mut World,
    controller: &mut dyn Controller,
    input: InputFrame,
    seconds: f32,
) -> f32 {
    let steps = (seconds * PHYSICS_HZ as f32).round() as usize;
    for _ in 0..steps {
        if world.is_control_step() {
            let ctx = ControlCtx::for_world(world, input);
            world.set_controller_throttles(controller.act(&ctx));
        }
        world.step(input);
    }
    world.air_used_kg()
}

/// Runs `episode` to its end with `controller` (reset for `seed` first).
fn fly_episode(mut episode: Episode, controller: &mut dyn Controller, seed: u64) -> EpisodeResult {
    controller.reset(seed);
    let mut throttles = [0.0; NOZZLE_COUNT];
    while step_episode(&mut episode, controller, &mut throttles) == EpisodeStatus::Running {}
    episode.result()
}

/// One physics step of `episode`, with `controller` acting at 60 Hz (SPEC §7.1) on what
/// `ControlCtx::for_episode` shows it; `throttles` holds its latest throttles between
/// control steps. Eval and the browser's training viewer both step episodes this way.
pub fn step_episode(
    episode: &mut Episode,
    controller: &mut dyn Controller,
    throttles: &mut [f32; NOZZLE_COUNT],
) -> EpisodeStatus {
    if episode.status() == EpisodeStatus::Running && episode.world().is_control_step() {
        let ctx = ControlCtx::for_episode(episode, InputFrame::default());
        *throttles = controller.act(&ctx);
    }
    episode.step(*throttles)
}

/// The landing-quality values of many episodes, gathered for percentiles (PLAN §3.3).
#[derive(Default)]
struct LandingSamples {
    hard_landing_episodes: usize,
    heading_errors_deg: Vec<f32>,
    speeds_kept: Vec<f32>,
    settle_times_s: Vec<f32>,
    air_left_at_success_kg: Vec<f32>,
}

impl LandingSamples {
    fn add(&mut self, result: &EpisodeResult) {
        if result.hard_landings > 0 {
            self.hard_landing_episodes += 1;
        }
        self.heading_errors_deg
            .extend(result.landing_heading_error_deg);
        self.speeds_kept.extend(result.speed_kept);
        self.settle_times_s.extend(result.settle_time_s);
        if result.success {
            self.air_left_at_success_kg.extend(result.air_left_kg);
        }
    }
}

/// Counts and touchdown samples gathered over many episodes, before they become rates and
/// percentiles.
#[derive(Default)]
struct EpisodeTally {
    spins: Vec<f32>,
    falls: Vec<f32>,
    successes: usize,
    saved: usize,
    crashed: usize,
    inverted: usize,
    total_saves: u32,
    total_air_kg: f32,
}

/// Walks `results` once, filling an [`EpisodeTally`] and the landing `samples`.
fn tally_episodes(results: &[EpisodeResult], landings: &mut LandingSamples) -> EpisodeTally {
    let mut tally = EpisodeTally::default();
    for result in results {
        landings.add(result);
        if result.success {
            tally.successes += 1;
        }
        if result.saves > 0 {
            tally.saved += 1;
        }
        if result.crash_cause.is_some() {
            tally.crashed += 1;
        }
        if result.peak_tilt_deg > INVERTED_TILT_DEG {
            tally.inverted += 1;
        }
        tally.spins.extend(result.touchdown_spin_rad_s);
        tally.falls.extend(result.touchdown_fall_m_s);
        tally.total_saves += result.saves;
        tally.total_air_kg += result.air_used_in_saves_kg;
    }
    tally
}

/// Summarises many episodes.
pub fn summarise(results: &[EpisodeResult]) -> EvalSummary {
    let mut landings = LandingSamples::default();
    let EpisodeTally {
        mut spins,
        mut falls,
        successes,
        saved,
        crashed,
        inverted,
        total_saves,
        total_air_kg,
    } = tally_episodes(results, &mut landings);
    let episodes = results.len();
    let rate = |count: usize| count as f32 / episodes.max(1) as f32;
    EvalSummary {
        episodes,
        success_rate: rate(successes),
        save_rate: rate(saved),
        crash_rate: rate(crashed),
        inversion_rate: rate(inverted),
        air_per_save_kg: (total_saves > 0).then(|| total_air_kg / total_saves as f32),
        touchdown_spin_median_rad_s: percentile(&mut spins, 0.5),
        touchdown_spin_p95_rad_s: percentile(&mut spins, 0.95),
        touchdown_fall_median_m_s: percentile(&mut falls, 0.5),
        touchdown_fall_p95_m_s: percentile(&mut falls, 0.95),
        hard_landing_rate: rate(landings.hard_landing_episodes),
        landing_heading_error_median_deg: percentile(&mut landings.heading_errors_deg, 0.5),
        speed_kept_median: percentile(&mut landings.speeds_kept, 0.5),
        settle_time_median_s: percentile(&mut landings.settle_times_s, 0.5),
        air_left_at_success_median_kg: percentile(&mut landings.air_left_at_success_kg, 0.5),
    }
}

/// The value below which `fraction` of `values` lie (nearest rank); None if empty.
fn percentile(values: &mut [f32], fraction: f32) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    let rank = ((values.len() - 1) as f32 * fraction).round() as usize;
    values.get(rank).copied()
}
