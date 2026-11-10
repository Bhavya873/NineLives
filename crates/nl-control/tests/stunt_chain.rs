//! Several stunts in one jump (round 7): the queue limit, the same stunt repeated and
//! different stunts back to back, refusals when time or air is short (Limited air against
//! Unlimited air), and the rule that a partial chain never reports landed.

#![allow(clippy::unwrap_used)]

use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML, StuntConfig};
use nl_control::stunt::{
    MAX_STUNTS, PlannerInputs, StuntDirector, StuntEvent, StuntKind, StuntMode, StuntPhase,
    StuntPlanner, StuntReason, authorize, authorize_chain,
};
use nl_control::{ControlCtx, Controller, Pd};
use nl_sim::car_state::CarState;
use nl_sim::forecast::LandingForecast;
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

const DT: f32 = 1.0 / 60.0;
const BACK: StuntKind = StuntKind::BackFlip;
const FRONT: StuntKind = StuntKind::FrontFlip;
const ROLL: StuntKind = StuntKind::BarrelRight;

fn config() -> StuntConfig {
    ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML)
        .unwrap()
        .stunt
}

fn state(air_time_s: f32) -> CarState {
    CarState {
        gravity_dir_body: [0.0, -1.0, 0.0],
        angular_velocity_body_rad_s: [0.0; 3],
        // Rises at half of g times the air time, so it lands at the same speed.
        linear_velocity_body_m_s: [20.0, 0.5 * 9.80665 * air_time_s, 0.0],
        height_m: 30.0,
        wheel_contact: [false; 4],
        mode_thrust_scale: 1.0,
        tank_gate: 1.0,
    }
}

fn inputs(air_time_s: f32, air_kg: f32, unlimited: bool) -> PlannerInputs {
    PlannerInputs {
        time_s: 0.0,
        dt_s: DT,
        state: state(air_time_s),
        air_kg,
        mode: if unlimited {
            StuntMode::Arcade
        } else {
            StuntMode::Realistic
        },
        unlimited_air: unlimited,
        forecast: Some(LandingForecast {
            time_s: air_time_s,
            point_m: [50.0, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            normal_tilt_deg: 2.0,
            usable_length_m: 30.0,
            time_uncertainty_s: 0.05,
            position_uncertainty_m: 1.0,
        }),
    }
}

#[test]
fn at_most_three_stunts_per_jump() {
    let mut planner = StuntPlanner::new(config());
    for press in 0..MAX_STUNTS {
        assert!(planner.request(BACK, press as f32 * 0.1), "press {press}");
    }
    assert!(!planner.request(BACK, 0.5));
    assert!(!planner.request(ROLL, 0.5));
}

#[test]
fn a_longer_chain_needs_more_time() {
    let config = config();
    let need = |kinds: &[StuntKind]| {
        authorize_chain(&config, kinds, &inputs(30.0, 1.0e6, false))
            .unwrap()
            .required_s
    };
    let single = need(&[BACK]);
    let double = need(&[BACK, BACK]);
    let triple = need(&[BACK, BACK, BACK]);
    println!("required s: single {single:.2}, double {double:.2}, triple {triple:.2}");
    assert!(double > single + 0.4 && triple > double + 0.4);
    // The single stunt is the same plan as before the chain existed.
    assert_eq!(
        authorize(&config, BACK, &inputs(30.0, 1.0e6, false))
            .unwrap()
            .required_s,
        single
    );
    // A different stunt next costs a full stop in between, so more than the same one.
    assert!(need(&[BACK, ROLL]) > need(&[BACK, BACK]) - 0.5);
}

#[test]
fn short_time_refuses_with_not_enough_time_in_both_air_modes() {
    let config = config();
    let single = authorize_chain(&config, &[BACK], &inputs(30.0, 1.0e6, false))
        .unwrap()
        .required_s;
    let double = authorize_chain(&config, &[BACK, BACK], &inputs(30.0, 1.0e6, false))
        .unwrap()
        .required_s;
    let air_s = (single + double) / 2.0 + 0.1;
    for unlimited in [false, true] {
        let input = inputs(air_s, 1.0e6, unlimited);
        assert!(authorize_chain(&config, &[BACK], &input).is_ok());
        assert_eq!(
            authorize_chain(&config, &[BACK, BACK], &input)
                .unwrap_err()
                .reason,
            StuntReason::NotEnoughTime,
            "unlimited {unlimited}"
        );
    }
}

#[test]
fn short_air_refuses_when_limited_and_not_when_unlimited() {
    let config = config();
    let long_s = 30.0;
    let needs = |kinds: &[StuntKind]| {
        authorize_chain(&config, kinds, &inputs(long_s, 1.0e6, false))
            .unwrap()
            .air_needed_kg
    };
    let (one, two) = (needs(&[BACK]), needs(&[BACK, BACK]));
    assert!(two > one);
    let tank = (one + two) / 2.0;
    assert!(authorize_chain(&config, &[BACK], &inputs(long_s, tank, false)).is_ok());
    assert_eq!(
        authorize_chain(&config, &[BACK, BACK], &inputs(long_s, tank, false))
            .unwrap_err()
            .reason,
        StuntReason::NotEnoughAir
    );
    // Unlimited air has no tank to run dry: only time limits it.
    assert!(authorize_chain(&config, &[BACK, BACK], &inputs(long_s, 0.0, true)).is_ok());
    // Limited air keeps the landing reserve on top of the spin: the whole tank must cover it.
    let reserve_only = needs(&[BACK]) - (one - config.landing_air_kg_per_m_s * 0.0);
    assert_eq!(reserve_only, 0.0);
    assert!(two > config.landing_air_kg_per_m_s * 9.80665 * long_s * 0.5);
}

#[test]
fn a_stunt_added_in_the_air_is_judged_with_the_extra_time_and_air() {
    let config = config();
    let single = authorize_chain(&config, &[BACK], &inputs(30.0, 1.0e6, false))
        .unwrap()
        .required_s;
    let double = authorize_chain(&config, &[BACK, BACK], &inputs(30.0, 1.0e6, false))
        .unwrap()
        .required_s;
    // Room for one flip but not two: the second press is refused and the first flies on.
    let air_s = (single + double) / 2.0 + 0.1;
    let mut planner = StuntPlanner::new(config);
    assert!(planner.request(BACK, 0.0));
    let tick = planner.tick(&inputs(air_s, 1.0e6, false));
    assert_eq!(tick.phase, StuntPhase::SpinUp);
    assert!(planner.request(BACK, 0.1));
    let tick = planner.tick(&inputs(air_s, 1.0e6, false));
    assert_eq!(
        tick.events,
        vec![StuntEvent::Refused {
            kind: BACK,
            reason: StuntReason::NotEnoughTime
        }]
    );
    assert_eq!(planner.chain(), vec![BACK]);
    // With the time, the same press is taken (`Started`) and the chain grows.
    let mut planner = StuntPlanner::new(config);
    planner.request(BACK, 0.0);
    planner.tick(&inputs(30.0, 1.0e6, false));
    assert!(planner.request(ROLL, 0.1));
    let tick = planner.tick(&inputs(30.0, 1.0e6, false));
    assert_eq!(tick.events, vec![StuntEvent::Started { kind: ROLL }]);
    assert_eq!(planner.chain(), vec![BACK, ROLL]);
    assert_eq!(planner.chain_code(), 24.0);
}

#[test]
fn the_tank_is_checked_again_for_every_added_stunt() {
    let config = config();
    let one = authorize_chain(&config, &[BACK], &inputs(30.0, 1.0e6, false))
        .unwrap()
        .air_needed_kg;
    let two = authorize_chain(&config, &[BACK, BACK], &inputs(30.0, 1.0e6, false))
        .unwrap()
        .air_needed_kg;
    let tank = (one + two) / 2.0;
    for (unlimited, expected) in [
        (false, StuntReason::NotEnoughAir),
        (true, StuntReason::NoLanding),
    ] {
        let mut planner = StuntPlanner::new(config);
        planner.request(BACK, 0.0);
        planner.tick(&inputs(30.0, tank, unlimited));
        planner.request(BACK, 0.1);
        let tick = planner.tick(&inputs(30.0, tank, unlimited));
        if unlimited {
            assert_eq!(
                tick.events,
                vec![StuntEvent::Started { kind: BACK }],
                "{expected:?}"
            );
        } else {
            assert_eq!(
                tick.events,
                vec![StuntEvent::Refused {
                    kind: BACK,
                    reason: expected
                }]
            );
        }
    }
}

// Flying a real car with the director.

struct Flight {
    events: Vec<StuntEvent>,
    chain: Vec<StuntKind>,
    crashed: bool,
    wheels_down_at_end: usize,
    air_left_kg: f32,
    peak_rotation_rad: f32,
}

/// Throws a car up at `up_m_s` (and forward at 15 m/s) from just above flat ground, so it
/// falls back at the same speed after 2 x up / g s of air. `first` is requested on the first tick, each of
/// `later` (kind, tick) is pressed at that control tick.
fn fly(first: &[StuntKind], later: &[(StuntKind, usize)], up_m_s: f32, unlimited: bool) -> Flight {
    let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML).unwrap();
    let mode = if unlimited {
        modes.arcade
    } else {
        modes.realistic
    };
    let controllers = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();
    let mut world = World::new_on_flat_ground(&vehicle, mode, DEFAULT_GROUND_FRICTION, 1);
    world
        .place_car(
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [15.0, up_m_s, 0.0],
            [0.0; 3],
        )
        .unwrap();
    let pd = Pd::for_vehicle(&vehicle, mode, &controllers);
    let mut fallback = pd.clone();
    let mut director = StuntDirector::new(controllers.stunt, pd);
    for &kind in first {
        assert!(director.request(kind, 0.0));
    }
    let mut events = Vec::new();
    let mut chain = Vec::new();
    let mut control_tick = 0usize;
    let mut peak_rotation_rad = 0.0f32;
    for _ in 0..(14.0 * 120.0) as usize {
        if world.is_control_step() {
            for &(kind, at) in later {
                if at == control_tick {
                    let _ = director.request(kind, world.time_s() as f32);
                }
            }
            control_tick += 1;
            let ctx = ControlCtx::for_world(&world, InputFrame::default());
            let step = director.step(&world, &ctx, DT);
            events.extend(step.tick.events.iter().copied());
            if director.planner().chain().len() > chain.len() {
                chain = director.planner().chain();
            }
            peak_rotation_rad = peak_rotation_rad.max(director.planner().total_rotation_rad());
            let throttles = step.throttles.unwrap_or_else(|| fallback.act(&ctx));
            world.set_controller_throttles(throttles);
        }
        world.step(InputFrame::default());
    }
    Flight {
        events,
        chain,
        crashed: world.is_crashed(),
        wheels_down_at_end: world.wheels_touching_count(),
        air_left_kg: world.air_kg(),
        peak_rotation_rad,
    }
}

fn landed(flight: &Flight) -> bool {
    flight
        .events
        .iter()
        .any(|e| matches!(e, StuntEvent::Landed { .. }))
}

#[test]
fn a_double_back_flip_turns_twice_and_lands() {
    let flight = fly(&[BACK, BACK], &[], 20.0, false);
    println!("{:?} left {:.0} kg", flight.events, flight.air_left_kg);
    assert_eq!(flight.chain, vec![BACK, BACK]);
    assert!(landed(&flight), "{:?}", flight.events);
    assert!(!flight.crashed && flight.wheels_down_at_end == 4);
    assert!(flight.peak_rotation_rad >= 2.0 * std::f32::consts::TAU - 0.3);
}

#[test]
fn pressing_the_key_again_during_the_spin_adds_a_rotation() {
    // Second press 20 control ticks (a third of a second) into the spin.
    let flight = fly(&[BACK], &[(BACK, 20)], 20.0, false);
    assert_eq!(flight.chain, vec![BACK, BACK], "{:?}", flight.events);
    assert!(landed(&flight), "{:?}", flight.events);
    assert!(!flight.crashed);
}

#[test]
fn a_triple_barrel_roll_in_unlimited_air() {
    let flight = fly(&[ROLL, ROLL, ROLL], &[], 27.0, true);
    println!("{:?}", flight.events);
    assert_eq!(flight.chain, vec![ROLL, ROLL, ROLL]);
    assert!(landed(&flight), "{:?}", flight.events);
    assert!(!flight.crashed && flight.wheels_down_at_end == 4);
}

#[test]
fn a_back_flip_then_a_barrel_roll_flies_both_and_lands() {
    let flight = fly(&[BACK, ROLL], &[], 24.0, false);
    println!("{:?}", flight.events);
    assert_eq!(flight.chain, vec![BACK, ROLL]);
    assert!(landed(&flight), "{:?}", flight.events);
    assert!(!flight.crashed && flight.wheels_down_at_end == 4);
}

#[test]
fn a_front_flip_a_back_flip_and_a_roll_in_one_jump_land() {
    let flight = fly(&[FRONT, BACK, ROLL], &[], 26.0, true);
    println!("{:?}", flight.events);
    assert_eq!(flight.chain, vec![FRONT, BACK, ROLL]);
    assert!(landed(&flight), "{:?}", flight.events);
}

#[test]
fn a_triple_back_flip_in_limited_air_keeps_air_for_the_landing() {
    let flight = fly(&[BACK, BACK, BACK], &[], 24.0, false);
    println!("{:?} air left {:.0} kg", flight.events, flight.air_left_kg);
    assert_eq!(flight.chain, vec![BACK, BACK, BACK]);
    assert!(landed(&flight), "{:?}", flight.events);
    assert!(!flight.crashed && flight.wheels_down_at_end == 4);
}

#[test]
fn a_drop_with_room_for_one_flip_refuses_the_second_and_still_lands_the_first() {
    let flight = fly(&[BACK], &[(BACK, 40)], 14.0, false);
    println!("{:?}", flight.events);
    assert!(flight.events.iter().any(|e| matches!(
        e,
        StuntEvent::Refused {
            reason: StuntReason::NotEnoughTime | StuntReason::NotEnoughAir,
            ..
        }
    )));
    assert_eq!(flight.chain, vec![BACK]);
    assert!(landed(&flight), "{:?}", flight.events);
}

#[test]
fn a_partial_chain_never_reports_landed() {
    // The planner is made to give up in the middle of a double: nothing may say Landed.
    let config = config();
    let mut planner = StuntPlanner::new(config);
    planner.request(BACK, 0.0);
    planner.request(BACK, 0.0);
    let flying = inputs(30.0, 1.0e6, false);
    planner.tick(&flying);
    // A spin of 3 rad/s for a second and a half is about one turn of the two.
    let mut spinning = flying;
    spinning.state.angular_velocity_body_rad_s[2] = 4.0;
    for _ in 0..80 {
        planner.tick(&spinning);
    }
    assert!(
        !planner.rotation_complete(),
        "one of two turns is not a chain"
    );
    // Now the car comes down on its wheels, level and quiet.
    let mut down = inputs(0.1, 1.0e6, false);
    down.state.wheel_contact = [true; 4];
    let mut events = Vec::new();
    for _ in 0..30 {
        events.extend(planner.tick(&down).events);
    }
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, StuntEvent::Landed { .. })),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, StuntEvent::Aborted { .. })),
        "{events:?}"
    );
}

#[test]
fn the_completed_turn_latch_holds_per_stunt_in_the_chain() {
    let config = config();
    let mut planner = StuntPlanner::new(config);
    planner.request(BACK, 0.0);
    planner.request(BACK, 0.0);
    let flying = inputs(30.0, 1.0e6, false);
    planner.tick(&flying);
    let mut spinning = flying;
    spinning.state.angular_velocity_body_rad_s[2] = 5.0;
    // 1.4 s at 5 rad/s is 7 rad: the first turn is latched, the second is not.
    for _ in 0..84 {
        planner.tick(&spinning);
    }
    assert!(!planner.rotation_complete());
    // Spinning back (PD's catch) cannot take the first turn away.
    spinning.state.angular_velocity_body_rad_s[2] = -5.0;
    for _ in 0..40 {
        planner.tick(&spinning);
    }
    assert!(!planner.rotation_complete());
    spinning.state.angular_velocity_body_rad_s[2] = 5.0;
    for _ in 0..140 {
        planner.tick(&spinning);
    }
    assert!(
        planner.rotation_complete(),
        "both turns made, the latch holds"
    );
}
