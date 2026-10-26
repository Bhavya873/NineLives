//! Eval runner tests (SPEC §10): episodes repeat for the same seed, and on Stage A drops
//! PD saves clearly more than Off (the M4 acceptance check, on a small sample).

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};
use nl_control::eval::{run_episode, summarise};
use nl_control::{Controller, Off, Pd};
use nl_sim::episode::EpisodeResult;
use nl_sim::events::CrashCause;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModeParams, ModesConfig};
use nl_sim::randomization::EpisodeVariation;
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig, Stage};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};

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
    let config = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();
    Pd::for_vehicle(&vehicle(), realistic(), &config)
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
                &EpisodeVariation::none(&vehicle(), realistic()),
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
    let episode = |success: bool, saves: u32, air_used_in_saves_kg: f32| EpisodeResult {
        seed: 0,
        success,
        saves,
        crash_cause: (!success).then_some(CrashCause::HardImpact),
        air_used_kg: 100.0,
        air_used_in_saves_kg,
        peak_tilt_deg: if success { 170.0 } else { 40.0 },
        touchdown_spin_rad_s: Some(1.0),
        touchdown_fall_m_s: Some(2.0),
        duration_s: 1.0,
        hard_landings: u32::from(!success),
        landing_heading_error_deg: Some(10.0),
        speed_kept: None,
        settle_time_s: success.then_some(0.8),
        air_left_kg: Some(200.0),
    };

    let summary = summarise(&[episode(true, 1, 30.0), episode(false, 0, 10.0)]);

    assert_eq!(summary.success_rate, 0.5);
    assert_eq!(summary.save_rate, 0.5);
    assert_eq!(summary.crash_rate, 0.5);
    // Only the 170° episode turned over.
    assert_eq!(summary.inversion_rate, 0.5);
    // Only air spent in saved upsets counts, not the 200 kg used overall.
    assert_eq!(summary.air_per_save_kg, Some(40.0));
    assert_eq!(summary.touchdown_fall_median_m_s, Some(2.0));
    assert_eq!(summary.hard_landing_rate, 0.5);
    assert_eq!(summary.landing_heading_error_median_deg, Some(10.0));
    assert_eq!(summary.speed_kept_median, None);
    assert_eq!(summary.settle_time_median_s, Some(0.8));
    assert_eq!(summary.air_left_at_success_median_kg, Some(200.0));
}

#[test]
fn crash_causes_are_written_in_snake_case() {
    let json = serde_json::to_string(&CrashCause::HardImpact).unwrap();

    assert_eq!(json, "\"hard_impact\"");
}
