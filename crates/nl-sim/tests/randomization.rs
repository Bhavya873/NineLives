//! Domain randomization tests (SPEC §9.8): draws repeat for a seed and stay inside their
//! ranges, latency delays the valves by whole control steps, observation noise has the
//! configured size, and a bad range is refused by name.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{realistic, scenarios, vehicle};
use nl_sim::car_state::CarState;
use nl_sim::episode::Episode;
use nl_sim::obs::{OBS_SPEC, PREV_ACTION_INDEX};
use nl_sim::randomization::{
    EpisodeVariation, ObsNoise, RandomizationConfig, add_state_noise, noise_rng, sample_variation,
};
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig, Stage};

#[test]
fn the_same_seed_gives_the_same_variation_and_another_seed_does_not() {
    let config = scenarios().randomization;

    let first = sample_variation(&config, &vehicle(), realistic(), 9);
    let second = sample_variation(&config, &vehicle(), realistic(), 9);
    let other = sample_variation(&config, &vehicle(), realistic(), 10);

    assert_eq!(first, second);
    assert_ne!(first, other);
}

#[test]
fn every_draw_stays_inside_its_range() {
    let config = scenarios().randomization;
    let base = vehicle();
    for seed in 0..200 {
        let variation = sample_variation(&config, &base, realistic(), seed);
        let mass_ratio = variation.vehicle.chassis.dry_mass_kg / base.chassis.dry_mass_kg;
        assert!((0.9..=1.1).contains(&mass_ratio));
        for axis in variation.vehicle.chassis.center_of_mass_offset_m {
            assert!(axis.abs() <= config.center_of_mass_offset_m);
        }
        for (varied, original) in variation
            .vehicle
            .thrusters
            .nozzles
            .iter()
            .zip(&base.thrusters.nozzles)
        {
            assert!((0.85..=1.15).contains(&(varied.max_force_n / original.max_force_n)));
        }
        assert!((0.0..=1.0).contains(&variation.wind.strength));
        assert!((0.0..=std::f32::consts::TAU).contains(&variation.wind.direction_yaw_rad));
        assert!((1.0..=2.0).contains(&variation.mode.thrust_scale));
        assert!((150.0..=350.0).contains(&variation.mode.tank_capacity_kg));
        assert!(variation.latency_control_steps <= 2);
        assert!((0.6..=1.2).contains(&variation.ground_friction));
    }
}

#[test]
fn about_forty_percent_of_episodes_get_unlimited_air() {
    let config = scenarios().randomization;

    let unlimited = (0..1000)
        .filter(|seed| {
            sample_variation(&config, &vehicle(), realistic(), *seed)
                .mode
                .unlimited_air
        })
        .count();

    assert!((330..=470).contains(&unlimited), "{unlimited} of 1000");
}

#[test]
fn a_two_step_latency_reaches_the_valves_two_control_steps_late() {
    let mut variation = EpisodeVariation::none(&vehicle(), realistic());
    variation.latency_control_steps = 2;
    let mut episode = Episode::start(&variation, &scenarios(), Stage::A, 4).unwrap();

    let mut valves = Vec::new();
    for _ in 0..3 {
        // One control step is two physics steps (SPEC §7.1).
        episode.step([1.0; 10]);
        episode.step([1.0; 10]);
        valves.push(episode.world().controller_throttles()[0]);
    }

    assert_eq!(valves, [0.0, 0.0, 1.0]);
}

#[test]
fn prev_action_shows_what_the_controller_asked_for_even_under_latency() {
    let mut variation = EpisodeVariation::none(&vehicle(), realistic());
    variation.latency_control_steps = 2;
    let mut episode = Episode::start(&variation, &scenarios(), Stage::A, 4).unwrap();
    let mut throttles = [0.0; 10];
    throttles[4] = 0.7;

    episode.step(throttles);
    let observation = episode.observe();

    assert_eq!(observation[PREV_ACTION_INDEX + 4], 0.7);
    assert_eq!(episode.world().controller_throttles()[4], 0.0);
}

#[test]
fn prev_action_row_is_where_the_observation_says() {
    assert_eq!(OBS_SPEC[PREV_ACTION_INDEX].name, "prev_action.l_fl");
}

#[test]
fn zero_noise_changes_nothing_and_noise_has_about_its_set_size() {
    let mut rng = noise_rng(1);
    let mut state = CarState::default();
    add_state_noise(&mut state, &ObsNoise::default(), &mut rng);
    assert_eq!(state, CarState::default());

    let noise = ObsNoise {
        spin_rad_s: 0.5,
        ..ObsNoise::default()
    };
    let mut sum_of_squares = 0.0;
    for _ in 0..4000 {
        let mut state = CarState::default();
        add_state_noise(&mut state, &noise, &mut rng);
        let spin_x = state.angular_velocity_body_rad_s[0];
        sum_of_squares += spin_x * spin_x;
        assert_eq!(state.gravity_dir_body, CarState::default().gravity_dir_body);
    }
    let std = (sum_of_squares / 4000.0_f32).sqrt();
    assert!((0.45..=0.55).contains(&std), "spin noise std {std}");
}

#[test]
fn a_thrust_scale_range_the_observation_cannot_show_is_refused() {
    let text =
        DEFAULT_SCENARIOS_TOML.replace("thrust_scale = [1.0, 2.0]", "thrust_scale = [1.0, 4.0]");

    let error = ScenariosConfig::from_toml_str(&text).unwrap_err();

    assert!(error.to_string().contains("thrust_scale"), "{error}");
}

#[test]
fn a_bad_randomization_range_is_refused_by_name() {
    let text = DEFAULT_SCENARIOS_TOML.replace(
        "unlimited_air_probability = 0.4",
        "unlimited_air_probability = 1.5",
    );

    let error = ScenariosConfig::from_toml_str(&text).unwrap_err();

    assert!(
        error.to_string().contains("unlimited_air_probability"),
        "{error}"
    );
}

#[test]
fn keeping_the_mode_keeps_the_preset_and_draws_everything_else_the_same() {
    let varying = scenarios().randomization;
    let keeping = RandomizationConfig {
        vary_mode: false,
        ..varying
    };

    let varied = sample_variation(&varying, &vehicle(), realistic(), 9);
    let kept = sample_variation(&keeping, &vehicle(), realistic(), 9);

    assert_eq!(kept.mode, realistic());
    assert_ne!(varied.mode, realistic());
    assert_eq!(kept.vehicle, varied.vehicle);
    assert_eq!(kept.ground_friction, varied.ground_friction);
    assert_eq!(kept.wind, varied.wind);
}
