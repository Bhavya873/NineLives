//! Stage F, the flight (SPEC §9.6), with the controller off so only the rules and the
//! driver bot act: the flip plate turns the car upside down, the bot keeps its hands on
//! the controls through the flight, and a seed always replays the same episode.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{realistic, scenarios, vehicle};
use nl_sim::conventions::{INVERTED_TILT_DEG, NOZZLE_COUNT};
use nl_sim::episode::{Episode, EpisodeStatus};
use nl_sim::randomization::EpisodeVariation;
use nl_sim::scenarios::Stage;

const NO_THRUST: [f32; NOZZLE_COUNT] = [0.0; NOZZLE_COUNT];
/// Observation index of the driver's throttle (`ObsSpec` field "driver_throttle").
const DRIVER_THROTTLE_INDEX: usize = 24;

fn start(seed: u64) -> Episode {
    let variation = EpisodeVariation::none(&vehicle(), realistic());
    Episode::start(&variation, &scenarios(), Stage::F, seed).unwrap()
}

fn run_to_end(episode: &mut Episode) -> EpisodeStatus {
    let mut status = EpisodeStatus::Running;
    while status == EpisodeStatus::Running {
        status = episode.step(NO_THRUST);
    }
    status
}

#[test]
fn with_the_thrusters_off_stage_f_turns_the_car_upside_down() {
    let mut inverted = 0;
    let mut crashed = 0;
    for seed in 0..20 {
        let mut episode = start(seed);

        let status = run_to_end(&mut episode);

        if episode.result().peak_tilt_deg > INVERTED_TILT_DEG {
            inverted += 1;
        }
        if matches!(status, EpisodeStatus::Crashed(_)) {
            crashed += 1;
        }
    }
    // Measured on 200 validation seeds: 100% past 150°, 98% crashed.
    assert!(inverted >= 18, "only {inverted} of 20 turned upside down");
    assert!(crashed >= 16, "only {crashed} of 20 crashed unaided");
}

#[test]
fn the_driver_bot_keeps_the_throttle_live_while_the_car_flies() {
    let mut episode = start(3);
    assert!(episode.driver().is_some(), "Stage F has a driver");
    let mut throttle_while_tipped = false;

    while episode.step(NO_THRUST) == EpisodeStatus::Running {
        let tilt_deg = episode.world().tilt_rad().to_degrees();
        let driver_throttle = episode.observe()[DRIVER_THROTTLE_INDEX];
        if tilt_deg > 90.0 && driver_throttle.abs() > 0.05 {
            throttle_while_tipped = true;
        }
    }

    assert!(
        throttle_while_tipped,
        "the AI must see the driver's throttle mid-flip"
    );
}

#[test]
fn the_same_seed_replays_the_same_flight() {
    let mut first = start(11);
    let mut second = start(11);

    let first_status = run_to_end(&mut first);
    let second_status = run_to_end(&mut second);

    assert_eq!(first_status, second_status);
    assert_eq!(first.result().duration_s, second.result().duration_s);
    assert_eq!(first.result().peak_tilt_deg, second.result().peak_tilt_deg);
}
