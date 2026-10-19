//! Reward tests (SPEC §9.4): each term has the sign and size its formula says, and
//! being alive never pays on its own (SPEC §9.11, the hover hack).

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_sim::reward::{DEFAULT_REWARD_TOML, RewardConfig, StepSummary, reward_terms};

fn config() -> RewardConfig {
    RewardConfig::from_toml_str(DEFAULT_REWARD_TOML).unwrap()
}

/// A car sitting still on its wheels with closed valves: nothing to reward or charge.
fn resting() -> StepSummary {
    StepSummary {
        up_alignment: 1.0,
        spin_rad_s: 0.0,
        landing_spin_rad_s: 0.0,
        landing_fall_m_s: 0.0,
        landing_heading_error_rad: None,
        landing_speed_kept: None,
        air_used_kg: 0.0,
        tank_capacity_kg: 260.0,
        unlimited_air: false,
        throttles: [0.0; 10],
        previous_throttles: [0.0; 10],
        airborne_s: 0.0,
        saved: false,
        crashed: false,
        hard_landings: 0,
        unsettled: false,
        succeeded: false,
        air_left_fraction: 1.0,
        height_m: 0.6,
        vertical_speed_m_s: 0.0,
    }
}

#[test]
fn resting_upright_with_closed_valves_earns_nothing() {
    let terms = reward_terms(&resting(), &config());

    assert_eq!(terms.total(), 0.0);
}

#[test]
fn upside_down_costs_twice_the_tilt_weight() {
    let step = StepSummary {
        up_alignment: -1.0,
        ..resting()
    };

    let terms = reward_terms(&step, &config());

    assert!((terms.tilt + 2.0 * 0.05).abs() < 1e-6);
}

#[test]
fn spin_cost_stops_growing_at_the_cap() {
    let fast = StepSummary {
        spin_rad_s: 10.0,
        ..resting()
    };
    let faster = StepSummary {
        spin_rad_s: 100.0,
        ..resting()
    };

    let fast_terms = reward_terms(&fast, &config());
    let faster_terms = reward_terms(&faster, &config());

    // |ω|² = 100 and 10 000 are both above the cap of 50: −0.002 × 50.
    assert!((fast_terms.spin + 0.1).abs() < 1e-6);
    assert_eq!(fast_terms.spin, faster_terms.spin);
}

#[test]
fn limited_air_costs_its_share_of_the_tank() {
    let step = StepSummary {
        air_used_kg: 26.0,
        ..resting()
    };

    let terms = reward_terms(&step, &config());

    // 26 kg of 260 kg is 10% of the tank: −8.0 × 0.1.
    assert!((terms.air + 0.8).abs() < 1e-6);
}

#[test]
fn unlimited_air_costs_by_throttle_instead() {
    let step = StepSummary {
        unlimited_air: true,
        air_used_kg: 26.0,
        throttles: [0.5; 10],
        previous_throttles: [0.5; 10],
        ..resting()
    };

    let terms = reward_terms(&step, &config());

    // Throttles sum to 5: −0.008 × 5. The kilograms do not count.
    assert!((terms.air + 0.04).abs() < 1e-6);
}

#[test]
fn airtime_is_free_for_half_a_second_then_costs_every_step() {
    let short = StepSummary {
        airborne_s: 0.4,
        ..resting()
    };
    let long = StepSummary {
        airborne_s: 0.6,
        ..resting()
    };

    assert_eq!(reward_terms(&short, &config()).airborne, 0.0);
    assert!((reward_terms(&long, &config()).airborne + 0.02).abs() < 1e-6);
}

#[test]
fn changing_throttles_costs_their_squared_change() {
    let mut throttles = [0.0; 10];
    throttles[0] = 1.0;
    let step = StepSummary {
        throttles,
        ..resting()
    };

    let terms = reward_terms(&step, &config());

    assert!((terms.smoothness + 0.002).abs() < 1e-6);
}

#[test]
fn a_still_soft_landing_save_pays_five_and_a_rough_one_pays_two() {
    let soft = StepSummary {
        saved: true,
        ..resting()
    };
    let rough = StepSummary {
        saved: true,
        landing_spin_rad_s: 5.0,
        landing_fall_m_s: 7.0,
        ..resting()
    };

    assert!((reward_terms(&soft, &config()).save - 5.0).abs() < 1e-6);
    assert!((reward_terms(&rough, &config()).save - 2.0).abs() < 1e-6);
}

#[test]
fn a_save_landing_turned_90_degrees_earns_less_than_one_landing_straight() {
    let straight = StepSummary {
        saved: true,
        landing_heading_error_rad: Some(0.0),
        ..resting()
    };
    let turned = StepSummary {
        saved: true,
        landing_heading_error_rad: Some(90_f32.to_radians()),
        ..resting()
    };

    let straight_save = reward_terms(&straight, &config()).save;
    let turned_save = reward_terms(&turned, &config()).save;

    // q has a clamp01(1 − error / 45°) factor: 1 straight, 0 at 90°.
    assert!((straight_save - 5.0).abs() < 1e-6);
    assert!((turned_save - 2.0).abs() < 1e-6);
}

#[test]
fn a_save_that_kept_half_its_speed_earns_half_the_quality_bonus() {
    let step = StepSummary {
        saved: true,
        landing_speed_kept: Some(0.5),
        ..resting()
    };

    // 2 + 3 × 0.5.
    assert!((reward_terms(&step, &config()).save - 3.5).abs() < 1e-6);
}

#[test]
fn each_hard_landing_costs_its_weight() {
    let step = StepSummary {
        hard_landings: 2,
        ..resting()
    };

    assert!((reward_terms(&step, &config()).hard_landing + 2.0).abs() < 1e-6);
}

#[test]
fn being_unsettled_after_touchdown_costs_its_weight_every_step() {
    // The repo sets this weight to 0 (gate-2 ablation); the rule still works when it isn't.
    let mut config = config();
    config.weights.unsettled = 0.01;
    let step = StepSummary {
        unsettled: true,
        ..resting()
    };

    assert!((reward_terms(&step, &config).unsettled + 0.01).abs() < 1e-6);
}

#[test]
fn air_left_pays_only_on_the_success_step() {
    let resting_with_air = StepSummary {
        air_left_fraction: 0.8,
        ..resting()
    };
    let succeeded = StepSummary {
        succeeded: true,
        ..resting_with_air
    };

    assert_eq!(reward_terms(&resting_with_air, &config()).air_left, 0.0);
    assert!((reward_terms(&succeeded, &config()).air_left - 0.8).abs() < 1e-6);
}

#[test]
fn unlimited_air_pays_no_air_left_bonus() {
    let step = StepSummary {
        succeeded: true,
        unlimited_air: true,
        ..resting()
    };

    assert_eq!(reward_terms(&step, &config()).air_left, 0.0);
}

#[test]
fn a_crash_costs_ten() {
    let step = StepSummary {
        crashed: true,
        ..resting()
    };

    assert!((reward_terms(&step, &config()).crash + 10.0).abs() < 1e-6);
}

#[test]
fn no_step_without_a_save_ever_pays() {
    // SPEC §9.4: no survival bonus. Hovering upright forever must never beat landing.
    for up_alignment in [-1.0, 0.0, 1.0] {
        for airborne_s in [0.0, 0.6, 5.0] {
            for throttle in [0.0, 0.5, 1.0] {
                for unlimited_air in [false, true] {
                    let step = StepSummary {
                        up_alignment,
                        airborne_s,
                        unlimited_air,
                        throttles: [throttle; 10],
                        air_used_kg: throttle,
                        ..resting()
                    };
                    assert!(reward_terms(&step, &config()).total() <= 0.0);
                }
            }
        }
    }
}

#[test]
fn a_negative_weight_is_refused_with_its_name() {
    let text = DEFAULT_REWARD_TOML.replace("tilt = 0.05", "tilt = -0.05");

    let error = RewardConfig::from_toml_str(&text).unwrap_err();

    assert!(error.to_string().contains("weights.tilt"), "{error}");
}

#[test]
fn a_zero_save_scale_is_refused_because_it_divides() {
    let text =
        DEFAULT_REWARD_TOML.replace("save_fall_scale_m_s = 6.0", "save_fall_scale_m_s = 0.0");

    let error = RewardConfig::from_toml_str(&text).unwrap_err();

    assert!(error.to_string().contains("save_fall_scale_m_s"), "{error}");
}

#[test]
fn success_pays_its_bonus_once_on_the_success_step_whatever_the_air() {
    let succeeded = StepSummary {
        succeeded: true,
        unlimited_air: true,
        ..resting()
    };

    assert_eq!(reward_terms(&resting(), &config()).success, 0.0);
    assert!((reward_terms(&succeeded, &config()).success - config().weights.success).abs() < 1e-6);
    assert!(config().weights.success > 0.0);
}

#[test]
fn thrust_while_steady_on_the_ground_costs_its_weight_per_unit_of_throttle() {
    let mut config = config();
    config.weights.ground_thrust = 0.05;
    let idling = StepSummary {
        throttles: [0.02; 10],
        ..resting()
    };

    assert!((reward_terms(&idling, &config).ground_thrust + 0.01).abs() < 1e-6);
}

#[test]
fn braking_a_fall_tilting_or_flying_high_is_never_charged_as_ground_thrust() {
    let mut config = config();
    config.weights.ground_thrust = 0.05;
    let firing = StepSummary {
        throttles: [0.5; 10],
        ..resting()
    };
    let falling = StepSummary {
        vertical_speed_m_s: -3.0,
        ..firing
    };
    let tilted = StepSummary {
        up_alignment: 0.8,
        ..firing
    };
    let high = StepSummary {
        height_m: 3.0,
        ..firing
    };
    let spinning = StepSummary {
        spin_rad_s: 2.0,
        ..firing
    };

    for step in [falling, tilted, high, spinning] {
        assert_eq!(reward_terms(&step, &config).ground_thrust, 0.0);
    }
}
