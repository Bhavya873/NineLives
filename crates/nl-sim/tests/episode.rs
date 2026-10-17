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
