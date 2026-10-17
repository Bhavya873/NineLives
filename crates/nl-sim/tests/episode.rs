//! Episode rule tests (SPEC §9.5): an episode ends on a crash, a held save or the time
//! limit, and stepping an ended episode changes nothing.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{realistic, vehicle};
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::episode::{Episode, EpisodeStatus};
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig, Stage};

const NO_THRUST: [f32; NOZZLE_COUNT] = [0.0; NOZZLE_COUNT];

fn run_to_end(episode: &mut Episode) -> EpisodeStatus {
    let mut status = EpisodeStatus::Running;
    while status == EpisodeStatus::Running {
        status = episode.step(NO_THRUST);
    }
    status
}

#[test]
fn the_time_limit_ends_an_episode_that_neither_crashes_nor_saves() {
    let mut scenarios = ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML).unwrap();
    scenarios.episode.time_limit_s = 0.1;
    let mut episode = Episode::start(&vehicle(), realistic(), &scenarios, Stage::B, 0).unwrap();

    let status = run_to_end(&mut episode);

    assert_eq!(status, EpisodeStatus::TimedOut);
    assert!((episode.result().duration_s - 0.1).abs() < 1e-3);
    assert!(!episode.result().success);
}

#[test]
fn an_ended_episode_ignores_further_steps() {
    let scenarios = ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML).unwrap();
    let mut episode = Episode::start(&vehicle(), realistic(), &scenarios, Stage::A, 3).unwrap();
    let status = run_to_end(&mut episode);
    let finished = episode.result();

    let again = episode.step(NO_THRUST);

    assert_eq!(again, status);
    assert_eq!(episode.result(), finished);
}

#[test]
fn a_car_resting_on_its_wheels_succeeds_after_the_hold_without_any_save() {
    let mut scenarios = ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML).unwrap();
    // A gentle drop: level, low, slow. It lands without an upset, so no Save fires.
    scenarios.stage_a.height_m = [1.0, 1.0];
    scenarios.stage_a.max_tilt_deg = 0.0;
    scenarios.stage_a.max_spin_rad_s = 0.0;
    scenarios.stage_a.horizontal_speed_m_s = [0.0, 0.0];
    scenarios.stage_a.vertical_speed_m_s = [0.0, 0.0];
    let mut episode = Episode::start(&vehicle(), realistic(), &scenarios, Stage::A, 0).unwrap();

    let status = run_to_end(&mut episode);

    assert_eq!(status, EpisodeStatus::Succeeded);
    let result = episode.result();
    assert_eq!(result.saves, 0);
    assert!(result.duration_s < 2.0, "{result:?}");
    assert!(result.touchdown_fall_m_s.is_some());
}
