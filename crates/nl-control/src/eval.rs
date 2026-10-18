//! The Rust eval runner (SPEC §10, M4): run a controller through many seeded episodes of
//! one stage and summarise how it did. No Python needed; the M5 Python eval steps the
//! same `nl_sim::episode::Episode`, so the numbers can be compared.

use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::episode::{Episode, EpisodeResult, EpisodeStatus};
use nl_sim::input::InputFrame;
use nl_sim::randomization::EpisodeVariation;
use nl_sim::scenarios::{ScenariosConfig, Stage};
use nl_sim::world::PlaceCarError;
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
    let mut episode = Episode::start(variation, scenarios, stage, seed)?;
    controller.reset(seed);
    let mut throttles = [0.0; NOZZLE_COUNT];
    while step_episode(&mut episode, controller, &mut throttles) == EpisodeStatus::Running {}
    Ok(episode.result())
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

/// Summarises many episodes.
pub fn summarise(results: &[EpisodeResult]) -> EvalSummary {
    let mut spins = Vec::new();
    let mut falls = Vec::new();
    let mut landings = LandingSamples::default();
    let mut successes = 0;
    let mut saved = 0;
    let mut crashed = 0;
    let mut total_saves = 0;
    let mut total_air_kg = 0.0;
    for result in results {
        landings.add(result);
        if result.success {
            successes += 1;
        }
        if result.saves > 0 {
            saved += 1;
        }
        if result.crash_cause.is_some() {
            crashed += 1;
        }
        spins.extend(result.touchdown_spin_rad_s);
        falls.extend(result.touchdown_fall_m_s);
        total_saves += result.saves;
        total_air_kg += result.air_used_in_saves_kg;
    }
    let episodes = results.len();
    let rate = |count: usize| count as f32 / episodes.max(1) as f32;
    EvalSummary {
        episodes,
        success_rate: rate(successes),
        save_rate: rate(saved),
        crash_rate: rate(crashed),
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
