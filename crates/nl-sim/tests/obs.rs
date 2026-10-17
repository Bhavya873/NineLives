//! Observation tests (SPEC §9.2, §12): the layout has 41 uniquely named values, its hash
//! is pinned, and the values describe the car the way their names say.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, UPSIDE_DOWN, new_world, run_and_collect};
use nl_sim::car_state::TERRAIN_PROBE_DISTANCES_M;
use nl_sim::input::InputFrame;
use nl_sim::obs::{OBS_DIM, OBS_SPEC, Observation, obs_schema_hash, observe};
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig, Stage, start_episode};

/// The index of the field called `name`.
fn index_of(name: &str) -> usize {
    OBS_SPEC
        .iter()
        .position(|field| field.name == name)
        .unwrap()
}

/// The value of the field called `name`.
fn value(observation: &Observation, name: &str) -> f32 {
    observation[index_of(name)]
}

#[test]
fn the_layout_has_41_uniquely_named_values() {
    assert_eq!(OBS_DIM, 41);
    for (index, field) in OBS_SPEC.iter().enumerate() {
        for other in &OBS_SPEC[index + 1..] {
            assert_ne!(field.name, other.name);
        }
        assert!(field.low < field.high, "{} has an empty range", field.name);
    }
}

#[test]
fn the_layout_follows_the_spec_row_order() {
    // SPEC §9.2 fixes these indices; a policy trained on one order is useless on another.
    assert_eq!(index_of("gravity_dir_body.x"), 0);
    assert_eq!(index_of("height"), 9);
    assert_eq!(index_of("wheel_contact.fl"), 10);
    assert_eq!(index_of("air_fraction"), 18);
    assert_eq!(index_of("driver_throttle"), 24);
    assert_eq!(index_of("terrain_probe.2m"), 26);
    assert_eq!(index_of("prev_action.l_fl"), 31);
    assert_eq!(index_of("prev_action.b_r"), 40);
}

#[test]
fn terrain_probe_names_match_their_distances() {
    let first = index_of("terrain_probe.2m");
    for (offset, distance_m) in TERRAIN_PROBE_DISTANCES_M.iter().enumerate() {
        assert_eq!(
            OBS_SPEC[first + offset].name,
            format!("terrain_probe.{distance_m}m")
        );
    }
}

#[test]
fn the_schema_hash_is_pinned() {
    // If this fails you changed the observation layout: every exported policy is now
    // stale. Update the pinned value on purpose and log it in docs/DECISIONS.md.
    assert_eq!(obs_schema_hash(), "16d2f5faa7964d05");
}

#[test]
fn a_car_resting_on_its_wheels_reads_level_grounded_and_full() {
    let mut world = new_world();
    run_and_collect(&mut world, 1.0);

    let observation = observe(&world, &world.car_state());

    assert!((value(&observation, "gravity_dir_body.y") + 1.0).abs() < 1e-3);
    assert!((value(&observation, "height") - 0.6).abs() < 0.1);
    for wheel in ["fl", "fr", "rl", "rr"] {
        assert_eq!(value(&observation, &format!("wheel_contact.{wheel}")), 1.0);
        let compression = value(&observation, &format!("suspension_compression.{wheel}"));
        assert!(
            compression > 0.1 && compression < 0.5,
            "{wheel}: {compression}"
        );
    }
    assert_eq!(value(&observation, "air_fraction"), 1.0);
    // Flat ground: every probe sees the ground about the resting height below.
    for distance in ["2m", "4m", "6m", "8m", "10m"] {
        let ground_m = value(&observation, &format!("terrain_probe.{distance}"));
        assert!((ground_m + 0.6).abs() < 0.1, "{distance}: {ground_m}");
    }
}

#[test]
fn an_upside_down_car_reads_gravity_up_and_no_wheel_contact() {
    let mut world = new_world();
    world
        .place_car([0.0, 3.0, 0.0], UPSIDE_DOWN, [0.0; 3], [0.0; 3])
        .unwrap();

    let observation = observe(&world, &world.car_state());

    assert!((value(&observation, "gravity_dir_body.y") - 1.0).abs() < 1e-3);
    for wheel in ["fl", "fr", "rl", "rr"] {
        assert_eq!(value(&observation, &format!("wheel_contact.{wheel}")), 0.0);
    }
}

#[test]
fn a_car_lifted_off_the_ground_reads_no_contact_before_the_next_step() {
    let mut world = new_world();
    run_and_collect(&mut world, 1.0);

    world
        .place_car([0.0, 5.0, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
    let observation = observe(&world, &world.car_state());

    for wheel in ["fl", "fr", "rl", "rr"] {
        assert_eq!(value(&observation, &format!("wheel_contact.{wheel}")), 0.0);
        let compression = value(&observation, &format!("suspension_compression.{wheel}"));
        assert_eq!(compression, 0.0);
    }
}

#[test]
fn the_last_throttles_and_driver_input_are_reported() {
    let mut world = new_world();
    world
        .place_car([0.0, 0.7, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
    let mut throttles = [0.0; 10];
    throttles[4] = 0.5;
    let input = InputFrame {
        throttle: 127,
        steer: -127,
        buttons: 0,
    };

    world.set_controller_throttles(throttles);
    world.step(input);
    let observation = observe(&world, &world.car_state());

    assert_eq!(value(&observation, "prev_action.s_fl"), 0.5);
    assert_eq!(value(&observation, "prev_action.l_fl"), 0.0);
    assert_eq!(value(&observation, "driver_throttle"), 1.0);
    assert_eq!(value(&observation, "driver_steer"), -1.0);
}

#[test]
fn values_stay_finite_and_inside_their_ranges_on_random_drops() {
    let config = ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML).unwrap();
    for seed in 0..30 {
        let mut world = new_world();
        start_episode(&mut world, Stage::A, &config, seed).unwrap();
        for _ in 0..240 {
            let observation = observe(&world, &world.car_state());
            for (index, field) in OBS_SPEC.iter().enumerate() {
                let reading = observation[index];
                assert!(
                    reading.is_finite(),
                    "seed {seed}: {} is {reading}",
                    field.name
                );
                assert!(reading >= field.low && reading <= field.high);
            }
            world.step(InputFrame::default());
        }
    }
}
