//! `episode::course_for_stage` hands the web the course a stage runs on (plan D1): none for
//! the flat-ground stages, the stage's own generated course otherwise, and flying that
//! course through `start_on_course` is the same episode `start` gives.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{realistic, scenarios, vehicle};
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::episode::{Episode, course_for_stage};
use nl_sim::randomization::EpisodeVariation;
use nl_sim::scenarios::Stage;

fn course(stage: Stage, seed: u64) -> Option<nl_sim::course::CourseSpec> {
    let plain = EpisodeVariation::none(&vehicle(), realistic());
    course_for_stage(&plain, &scenarios(), stage, seed).unwrap()
}

#[test]
fn flat_ground_stages_have_no_course() {
    assert!(course(Stage::A, 3).is_none());
    assert!(course(Stage::B, 3).is_none());
}

#[test]
fn the_driving_and_flight_stages_have_a_course_that_is_the_same_for_a_seed() {
    for stage in [Stage::C, Stage::D, Stage::F] {
        let first = course(stage, 5).unwrap();
        let second = course(stage, 5).unwrap();

        assert_eq!(first.to_json().unwrap(), second.to_json().unwrap());
    }
    // The generated courses have pieces; Stage C's track may be bare.
    assert!(!course(Stage::D, 5).unwrap().pieces.is_empty());
    assert!(!course(Stage::F, 5).unwrap().pieces.is_empty());
    let other = course(Stage::D, 6).unwrap();
    assert_ne!(
        course(Stage::D, 5).unwrap().to_json().unwrap(),
        other.to_json().unwrap()
    );
}

#[test]
fn flying_the_returned_course_is_the_episode_start_gives() {
    let plain = EpisodeVariation::none(&vehicle(), realistic());
    for stage in [Stage::D, Stage::F] {
        let spec = course(stage, 9).unwrap();
        let mut natural = Episode::start(&plain, &scenarios(), stage, 9).unwrap();
        let mut on_spec = Episode::start_on_course(&plain, &scenarios(), stage, &spec, 9).unwrap();

        for _ in 0..600 {
            natural.step([0.0; NOZZLE_COUNT]);
            on_spec.step([0.0; NOZZLE_COUNT]);
        }

        assert_eq!(natural.result(), on_spec.result(), "{stage:?}");
        assert_eq!(
            natural.world().chassis_pose(),
            on_spec.world().chassis_pose(),
            "{stage:?}"
        );
    }
}
