//! Eval runner tests (SPEC §10): episodes repeat for the same seed, and on Stage A drops
//! PD saves clearly more than Off (the M4 acceptance check, on a small sample).

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};
use nl_control::eval::{EpisodeResult, run_episode, summarise};
use nl_control::{Controller, Off, Pd};
use nl_sim::modes::{DEFAULT_MODES_TOML, ModeParams, ModesConfig};
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig, Stage};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

fn vehicle() -> VehicleConfig {
    VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap()
}

fn realistic() -> ModeParams {
    ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
        .unwrap()
        .realistic
}

fn scenarios() -> ScenariosConfig {
    ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML).unwrap()
}

fn pd() -> Pd {
    let world = World::new_on_flat_ground(&vehicle(), realistic(), DEFAULT_GROUND_FRICTION, 0);
    let config = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();
    Pd::new(&world.car_model(), &config)
}

fn run(
    stage: Stage,
    controller: &mut dyn Controller,
    seeds: std::ops::Range<u64>,
) -> Vec<EpisodeResult> {
    let mut results = Vec::new();
    for seed in seeds {
        results.push(
            run_episode(
                &vehicle(),
                realistic(),
                &scenarios(),
                stage,
                controller,
                seed,
            )
            .unwrap(),
        );
    }
    results
}

#[test]
fn the_same_seed_gives_the_same_episode() {
    let first = run(Stage::A, &mut pd(), 7..8);
    let second = run(Stage::A, &mut pd(), 7..8);

    assert_eq!(first, second);
}

#[test]
fn pd_saves_clearly_more_stage_a_drops_than_off() {
    let off = summarise(&run(Stage::A, &mut Off, 0..40));
    let pd = summarise(&run(Stage::A, &mut pd(), 0..40));

    assert!(
        pd.success_rate > off.success_rate + 0.3,
        "PD {pd:?}\nOff {off:?}"
    );
}

#[test]
fn pd_rights_stage_b_tip_overs() {
    let pd = summarise(&run(Stage::B, &mut pd(), 0..20));

    assert!(pd.success_rate >= 0.9, "{pd:?}");
}

#[test]
fn the_summary_counts_rates_and_air_per_save() {
    let episode = |success: bool, saves: u32, air_used_kg: f32| EpisodeResult {
        seed: 0,
        success,
        saves,
        crashes: u32::from(!success),
        crash_cause: None,
        air_used_kg,
        peak_tilt_deg: 0.0,
        touchdown_spin_rad_s: Some(1.0),
        touchdown_fall_m_s: Some(2.0),
        duration_s: 1.0,
    };

    let summary = summarise(&[episode(true, 1, 30.0), episode(false, 0, 10.0)]);

    assert_eq!(summary.success_rate, 0.5);
    assert_eq!(summary.crash_rate, 0.5);
    assert_eq!(summary.air_per_save_kg, Some(40.0));
    assert_eq!(summary.touchdown_fall_median_m_s, Some(2.0));
}
