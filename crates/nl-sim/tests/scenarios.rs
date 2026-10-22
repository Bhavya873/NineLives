//! Scenario tests (SPEC §9.6): starts repeat for the same seed, stay inside their
//! ranges, and the config refuses a backwards range or a negative maximum.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{new_world, scenarios};
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig, Stage, start_episode};

#[test]
fn the_same_seed_gives_the_same_start_and_another_seed_does_not() {
    let config = scenarios();
    let mut first = new_world();
    let mut second = new_world();
    let mut other = new_world();

    start_episode(&mut first, Stage::A, &config, 42).unwrap();
    start_episode(&mut second, Stage::A, &config, 42).unwrap();
    start_episode(&mut other, Stage::A, &config, 43).unwrap();

    assert_eq!(first.chassis_pose(), second.chassis_pose());
    assert_eq!(first.linear_velocity_m_s(), second.linear_velocity_m_s());
    assert_ne!(first.chassis_pose(), other.chassis_pose());
}

#[test]
fn stage_a_drops_start_in_the_air_within_their_ranges_with_a_full_tank() {
    let config = scenarios();
    for seed in 0..50 {
        let mut world = new_world();
        world.set_air_kg(0.0);

        start_episode(&mut world, Stage::A, &config, seed).unwrap();

        // At least the drawn height, raised if a tilted corner would touch the ground.
        let [low_m, _] = config.stage_a.height_m;
        let height_m = world.chassis_pose().position_m[1];
        assert!(height_m >= low_m, "seed {seed}: {height_m} m");
        let gap_m = world.lowest_chassis_point_m();
        assert!(
            gap_m >= config.episode.spawn_clearance_m - 1e-3,
            "seed {seed}"
        );
        let [_, vertical_m_s, _] = world.linear_velocity_m_s();
        let [down_m_s, up_m_s] = config.stage_a.vertical_speed_m_s;
        assert!((down_m_s..=up_m_s).contains(&vertical_m_s), "seed {seed}");
        assert_eq!(world.air_fraction(), 1.0);
        // The tilt is exactly the drawn angle, up to the configured maximum.
        assert!(world.tilt_rad().to_degrees() <= config.stage_a.max_tilt_deg + 0.01);
    }
}

#[test]
fn stage_b_starts_tipped_onto_a_side() {
    let config = scenarios();
    for seed in 0..50 {
        let mut world = new_world();

        start_episode(&mut world, Stage::B, &config, seed).unwrap();

        // Rolled 60°–110° (plus a little pitch): the up axis is far from world up.
        let up_alignment = world.car_state().up_alignment();
        assert!(up_alignment < 0.55, "seed {seed}: {up_alignment}");
        assert!(up_alignment > -0.40, "seed {seed}: {up_alignment}");
        // Just above the ground, not inside it.
        let gap_m = world.lowest_chassis_point_m();
        let clearance_m = config.episode.spawn_clearance_m;
        assert!((gap_m - clearance_m).abs() < 1e-3, "seed {seed}: {gap_m} m");
    }
}

#[test]
fn a_backwards_range_is_refused_with_its_field_name() {
    let broken = DEFAULT_SCENARIOS_TOML.replace("height_m = [1.5, 8.0]", "height_m = [8.0, 1.5]");

    let error = ScenariosConfig::from_toml_str(&broken).unwrap_err();

    assert!(error.to_string().contains("stage_a.height_m"), "{error}");
}

#[test]
fn a_negative_height_is_refused_with_its_field_name() {
    let broken = DEFAULT_SCENARIOS_TOML.replace("height_m = [1.5, 8.0]", "height_m = [-1.0, 8.0]");

    let error = ScenariosConfig::from_toml_str(&broken).unwrap_err();

    assert!(error.to_string().contains("stage_a.height_m"), "{error}");
}

#[test]
fn a_negative_maximum_is_refused_with_its_field_name() {
    let broken = DEFAULT_SCENARIOS_TOML.replace("max_spin_rad_s = 6.0", "max_spin_rad_s = -1.0");

    let error = ScenariosConfig::from_toml_str(&broken).unwrap_err();

    assert!(
        error.to_string().contains("stage_a.max_spin_rad_s"),
        "{error}"
    );
}

#[test]
fn stage_names_read_either_case() {
    assert_eq!(Stage::from_name("a"), Some(Stage::A));
    assert_eq!(Stage::from_name("B"), Some(Stage::B));
    assert_eq!(Stage::from_name("c"), Some(Stage::C));
    assert_eq!(Stage::from_name("E"), Some(Stage::E));
    assert_eq!(Stage::from_name("f"), Some(Stage::F));
    assert_eq!(Stage::from_name("G"), None);
}

#[test]
fn a_driver_bot_turn_limit_of_zero_or_less_is_refused() {
    for bad in ["0.0", "-6.0"] {
        let text = DEFAULT_SCENARIOS_TOML.replace(
            "max_turn_accel_m_s2 = 6.0",
            &format!("max_turn_accel_m_s2 = {bad}"),
        );

        let error = ScenariosConfig::from_toml_str(&text).unwrap_err();

        assert!(
            error.to_string().contains("driver_bot.max_turn_accel_m_s2"),
            "{error}"
        );
    }
}

#[test]
fn a_stage_f_plate_too_far_for_the_success_hold_is_refused() {
    // 12 m at the slowest 10 m/s takes 1.2 s: a quiet car would "succeed" before the plate.
    let broken = DEFAULT_SCENARIOS_TOML.replace("plate_ahead_m = 5.0", "plate_ahead_m = 12.0");

    let error = ScenariosConfig::from_toml_str(&broken).unwrap_err();

    assert!(
        error.to_string().contains("stage_f.plate_ahead_m"),
        "{error}"
    );
}
