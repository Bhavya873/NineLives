//! The driving stages run end to end with the PD controller (SPEC §9.5–§9.7, §13 M7):
//! the driver bot reaches the finish of nearly every generated course, Stage C hops and
//! saves, Stage E's knocks still leave most episodes savable, and a seed always replays
//! the same episode.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use std::ops::Range;

use nl_control::config::DEFAULT_CONTROLLERS_TOML;
use nl_control::eval::{run_episode, step_episode};
use nl_control::{ControllerKind, build_controller};
use nl_sim::episode::{Episode, EpisodeResult, EpisodeStatus};
use nl_sim::modes::{DEFAULT_MODES_TOML, ModeParams, ModesConfig};
use nl_sim::randomization::EpisodeVariation;
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig, Stage};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};

fn vehicle() -> VehicleConfig {
    VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap()
}

fn mode(name: &str) -> ModeParams {
    ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
        .unwrap()
        .by_name(name)
        .unwrap()
}

fn scenarios() -> ScenariosConfig {
    ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML).unwrap()
}

fn pd(mode: ModeParams) -> ControllerKind {
    build_controller("pd", &vehicle(), mode, DEFAULT_CONTROLLERS_TOML, None).unwrap()
}

/// Runs `seeds` of `stage` in `mode_name` with PD and no randomization.
fn run(stage: Stage, mode_name: &str, seeds: Range<u64>) -> Vec<EpisodeResult> {
    let mode = mode(mode_name);
    let variation = EpisodeVariation::none(&vehicle(), mode);
    let mut controller = pd(mode);
    seeds
        .map(|seed| run_episode(&variation, &scenarios(), stage, &mut controller, seed).unwrap())
        .collect()
}

fn success_count(results: &[EpisodeResult]) -> usize {
    results.iter().filter(|result| result.success).count()
}

/// One line per failed episode, for assertion messages.
fn failures(results: &[EpisodeResult]) -> String {
    let mut lines = Vec::new();
    for result in results.iter().filter(|result| !result.success) {
        lines.push(format!(
            "seed {}: crash {:?} after {:.1} s",
            result.seed, result.crash_cause, result.duration_s
        ));
    }
    lines.join("\n")
}

#[test]
fn the_bot_with_pd_finishes_at_least_nine_in_ten_generated_courses() {
    // Measured: 98 of seeds 0–99 in both modes.
    let results = run(Stage::D, "realistic", 0..50);

    assert!(success_count(&results) >= 45, "{}", failures(&results));
}

#[test]
fn stage_c_hops_and_saves_with_pd_in_both_modes() {
    // Measured: 100 of seeds 0–99 in both modes.
    for mode_name in ["realistic", "arcade"] {
        let results = run(Stage::C, mode_name, 0..30);

        assert!(
            success_count(&results) >= 27,
            "{mode_name}:\n{}",
            failures(&results)
        );
        assert!(
            results.iter().all(|result| result.saves >= 1),
            "{mode_name}: an episode without a save:\n{}",
            failures(&results)
        );
    }
}

#[test]
fn most_stage_e_episodes_survive_their_knocks_with_pd() {
    // Measured: 82 of seeds 0–99 (the base A drops are the hardest).
    let results = run(Stage::E, "realistic", 0..30);

    assert!(success_count(&results) >= 20, "{}", failures(&results));
}

/// Runs one episode with PD and returns its row and the car's final position.
fn replay(stage: Stage, seed: u64) -> (EpisodeResult, [f32; 3]) {
    let mode = mode("realistic");
    let variation = EpisodeVariation::none(&vehicle(), mode);
    let mut episode = Episode::start(&variation, &scenarios(), stage, seed).unwrap();
    let mut controller = pd(mode);
    let mut throttles = [0.0; 10];
    while step_episode(&mut episode, &mut controller, &mut throttles) == EpisodeStatus::Running {}
    (episode.result(), episode.world().chassis_pose().position_m)
}

#[test]
fn a_stage_c_or_d_episode_replays_exactly_from_its_seed() {
    for stage in [Stage::C, Stage::D] {
        let first = replay(stage, 11);
        let second = replay(stage, 11);

        assert_eq!(first, second, "{stage:?}");
    }
}
