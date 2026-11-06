//! The widened Stage A and F draws (scenarios.toml `[widening]`): they come from their own
//! random streams, so switching them off gives back the old episodes, and the episodes the
//! widening leaves alone start exactly as before.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{realistic, vehicle};
use nl_sim::episode::Episode;
use nl_sim::randomization::EpisodeVariation;
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig, Stage};
use nl_sim::spin_axis::SpinAxis;

fn scenarios() -> ScenariosConfig {
    ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML).unwrap()
}

fn start(scenarios: &ScenariosConfig, stage: Stage, seed: u64) -> Episode {
    let variation = EpisodeVariation::none(&vehicle(), realistic());
    Episode::start(&variation, scenarios, stage, seed).unwrap()
}

#[test]
fn the_widening_is_on_by_default_and_legacy_turns_it_off() {
    let config = scenarios();

    assert!(config.widened().is_some());
    assert!(config.legacy().widened().is_none());
}

#[test]
fn stage_a_drops_the_widening_does_not_script_start_as_they_always_did() {
    let widened = scenarios();
    let legacy = widened.legacy();
    let mut plain = 0;
    let mut scripted = 0;
    for seed in 0..60 {
        let new = start(&widened, Stage::A, seed);
        let old = start(&legacy, Stage::A, seed);

        if new.result().preset.is_none() {
            plain += 1;
            assert_eq!(new.world().chassis_pose(), old.world().chassis_pose());
            assert_eq!(new.car_state(), old.car_state());
        } else {
            scripted += 1;
            let rate = new.result().strength.unwrap();
            assert!((3.0..=10.0).contains(&rate), "{rate}");
        }
        assert_eq!(old.result().preset, None);
    }
    assert!(
        plain > 0 && scripted > 0,
        "{plain} plain, {scripted} scripted"
    );
}

#[test]
fn scripted_flips_spin_about_the_side_and_rolls_about_the_nose() {
    let config = scenarios();
    let mut seen = Vec::new();
    for seed in 0..200 {
        let result = start(&config, Stage::A, seed).result();
        let (Some(name), Some(axis)) = (result.preset, result.spin_axis) else {
            continue;
        };
        match name.as_str() {
            "scripted_flip" => assert_eq!(axis, SpinAxis::Pitch),
            "scripted_roll" => assert_eq!(axis, SpinAxis::Roll),
            "scripted_tumble" => {}
            other => panic!("unexpected preset {other}"),
        }
        if !seen.contains(&name) {
            seen.push(name);
        }
    }
    assert_eq!(seen.len(), 3, "{seen:?}");
}

#[test]
fn easy_curriculum_levels_get_no_scripted_entries() {
    let config = scenarios().at_level(Stage::A, 0);

    for seed in 0..100 {
        assert_eq!(start(&config, Stage::A, seed).result().preset, None);
    }
}

#[test]
fn stage_f_keeps_its_speed_and_gains_presets_strengths_and_a_start_attitude() {
    let widened = scenarios();
    let legacy = widened.legacy();
    let [weak, strong] = widened.widened().unwrap().stage_f.strength;
    let mut names = Vec::new();
    let mut tilted = 0;
    for seed in 0..100 {
        let new = start(&widened, Stage::F, seed);
        let old = start(&legacy, Stage::F, seed);

        let speed = |episode: &Episode| {
            let [x, y, z] = episode.car_state().linear_velocity_body_m_s;
            (x * x + y * y + z * z).sqrt()
        };
        assert!((speed(&new) - speed(&old)).abs() < 1e-3, "seed {seed}");
        let result = new.result();
        let strength = result.strength.unwrap();
        assert!((weak..=strong).contains(&strength), "{strength}");
        if result.spin_axis == Some(SpinAxis::Mixed) {
            assert_eq!(result.preset.as_deref(), Some("roll_flip"));
        }
        let name = result.preset.unwrap();
        if !names.contains(&name) {
            names.push(name);
        }
        tilted += usize::from(new.car_state().up_alignment() < 0.9999);
        assert!(old.car_state().up_alignment() > 0.9999);
    }
    // front and back flip, both barrels, the roof toss and the mixed plate.
    assert_eq!(names.len(), 6, "{names:?}");
    assert!(tilted > 90, "{tilted} of 100 started off level");
}

#[test]
fn stage_f_rows_name_the_legacy_preset_and_axis_too() {
    let legacy = scenarios().legacy();

    let result = start(&legacy, Stage::F, 3).result();

    assert!(result.preset.is_some() && result.strength.is_some());
    assert!(matches!(
        result.spin_axis,
        Some(SpinAxis::Roll | SpinAxis::Pitch)
    ));
}

#[test]
fn other_stages_carry_no_preset_and_stage_e_keeps_the_plain_drops() {
    let config = scenarios();

    for seed in 0..40 {
        for stage in [Stage::B, Stage::C, Stage::D, Stage::E] {
            let result = start(&config, stage, seed).result();
            assert_eq!(result.preset, None, "{stage:?} {seed}");
        }
    }
}
