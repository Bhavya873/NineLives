//! Episode rule tests (SPEC §9.5): an episode ends on a crash, a held save or the time
//! limit, and stepping an ended episode changes nothing.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{realistic, vehicle};
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::episode::{Episode, EpisodeStatus};
use nl_sim::randomization::EpisodeVariation;
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
    let mut episode = Episode::start(
        &EpisodeVariation::none(&vehicle(), realistic()),
        &scenarios,
        Stage::B,
        0,
    )
    .unwrap();

    let status = run_to_end(&mut episode);

    assert_eq!(status, EpisodeStatus::TimedOut);
    assert!((episode.result().duration_s - 0.1).abs() < 1e-3);
    assert!(!episode.result().success);
}

#[test]
fn an_ended_episode_ignores_further_steps() {
    let scenarios = ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML).unwrap();
    let mut episode = Episode::start(
        &EpisodeVariation::none(&vehicle(), realistic()),
        &scenarios,
        Stage::A,
        3,
    )
    .unwrap();
    let status = run_to_end(&mut episode);
    let finished = episode.result();

    let again = episode.step(NO_THRUST);

    assert_eq!(again, status);
    assert_eq!(episode.result(), finished);
}

/// A level Stage A drop from `height_m` with no spin or speed, at rest.
fn level_drop(height_m: f32) -> Episode {
    let mut scenarios = ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML).unwrap();
    scenarios.stage_a.height_m = [height_m, height_m];
    scenarios.stage_a.max_tilt_deg = 0.0;
    scenarios.stage_a.max_spin_rad_s = 0.0;
    scenarios.stage_a.horizontal_speed_m_s = [0.0, 0.0];
    scenarios.stage_a.vertical_speed_m_s = [0.0, 0.0];
    let variation = EpisodeVariation::none(&vehicle(), realistic());
    Episode::start(&variation, &scenarios, Stage::A, 0).unwrap()
}

#[test]
fn a_car_resting_on_its_wheels_succeeds_after_the_hold_without_any_save() {
    // A gentle drop: level, low, slow. It lands without an upset, so no Save fires.
    let mut episode = level_drop(1.0);

    let status = run_to_end(&mut episode);

    assert_eq!(status, EpisodeStatus::Succeeded);
    let result = episode.result();
    assert_eq!(result.saves, 0);
    assert!(result.duration_s < 2.0, "{result:?}");
    assert!(result.touchdown_fall_m_s.is_some());
    assert_eq!(result.hard_landings, 0);
    assert_eq!(result.settle_time_s, Some(0.0));
}

#[test]
fn a_level_drop_from_3_m_bottoms_out_the_springs_a_hard_landing_but_no_crash() {
    let mut episode = level_drop(3.0);

    let status = run_to_end(&mut episode);

    assert_eq!(status, EpisodeStatus::Succeeded);
    assert_eq!(episode.result().hard_landings, 1);
}

#[test]
fn a_parked_car_blowing_a_throttle_sum_of_0_2_does_not_count_as_success() {
    let mut episode = level_drop(1.0);
    let idle_thrust = [0.02; NOZZLE_COUNT];

    let mut status = EpisodeStatus::Running;
    while status == EpisodeStatus::Running {
        status = episode.step(idle_thrust);
    }

    // Four wheels were down long enough, but the valves never closed.
    assert_eq!(status, EpisodeStatus::TimedOut);
    assert!(!episode.result().success);
}

#[test]
fn closing_the_valves_after_landing_succeeds_half_a_second_later() {
    let mut episode = level_drop(1.0);
    let idle_thrust = [0.02; NOZZLE_COUNT];
    for _ in 0..240 {
        episode.step(idle_thrust);
    }
    let still_running = episode.status();

    let status = run_to_end(&mut episode);

    assert_eq!(still_running, EpisodeStatus::Running);
    assert_eq!(status, EpisodeStatus::Succeeded);
    let quiet_s = episode.result().duration_s - 2.0;
    assert!((0.49..0.52).contains(&quiet_s), "{quiet_s}");
}

#[test]
fn the_quiet_throttle_rule_never_turns_a_crash_into_anything_else() {
    // An 8 m level drop with closed valves hits the ground too hard.
    let mut episode = level_drop(8.0);

    let status = run_to_end(&mut episode);

    assert!(matches!(status, EpisodeStatus::Crashed(_)), "{status:?}");
    let result = episode.result();
    assert!(!result.success);
    assert_eq!(result.settle_time_s, None);
}

#[test]
fn near_zero_throttles_under_the_deadband_count_as_quiet() {
    let mut episode = level_drop(1.0);
    // Every nozzle at 0.015: a sum of 0.15, but each below the 0.02 deadband.
    let trickle = [0.015; NOZZLE_COUNT];

    let mut status = EpisodeStatus::Running;
    while status == EpisodeStatus::Running {
        status = episode.step(trickle);
    }

    assert_eq!(status, EpisodeStatus::Succeeded);
}

#[test]
fn one_nozzle_really_firing_still_breaks_the_quiet_rule() {
    let mut episode = level_drop(1.0);
    let mut one_firing = [0.0; NOZZLE_COUNT];
    one_firing[5] = 0.06;

    let mut status = EpisodeStatus::Running;
    while status == EpisodeStatus::Running {
        status = episode.step(one_firing);
    }

    assert_eq!(status, EpisodeStatus::TimedOut);
}
