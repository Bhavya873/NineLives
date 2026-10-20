//! Air tank, Hop and Boost tests (SPEC §7.5–7.8 and §12), run in the full world so
//! valve lag, thrust scale and the tank gate are all included.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, arcade, buttons, new_world, new_world_in, realistic};
use nl_sim::conventions::{GRAVITY_M_S2, NOZZLE_COUNT, PHYSICS_HZ};
use nl_sim::hop_boost::HopBlockedReason;
use nl_sim::input::{BUTTON_BOOST, BUTTON_HOP, InputFrame};
use nl_sim::world::World;

const STEPS_PER_SECOND: usize = PHYSICS_HZ as usize;

fn run(world: &mut World, input: InputFrame, seconds: f32) {
    for _ in 0..(seconds * STEPS_PER_SECOND as f32).round() as usize {
        world.step(input);
    }
}

/// Puts the car high in the air, level and still.
fn lift_into_the_air(world: &mut World, height_m: f32) {
    world
        .place_car([0.0, height_m, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
}

/// Lets the car settle on its wheels and returns its resting centre-of-mass height.
fn settle(world: &mut World) -> f32 {
    run(world, InputFrame::default(), 1.5);
    world.chassis_pose().position_m[1]
}

/// Holds Hop for one step to start it, then records the highest the car gets.
fn hop_apex_rise_m(world: &mut World) -> f32 {
    let rest_height_m = settle(world);
    world.step(buttons(BUTTON_HOP));
    let mut highest_m = rest_height_m;
    for _ in 0..4 * STEPS_PER_SECOND {
        world.step(InputFrame::default());
        highest_m = highest_m.max(world.chassis_pose().position_m[1]);
    }
    highest_m - rest_height_m
}

#[test]
fn one_second_of_full_lift_uses_80_kg_of_air() {
    let mut world = new_world();
    lift_into_the_air(&mut world, 100.0);
    let lift = [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    world.set_controller_throttles(lift);
    // Let the valves open fully first; the lag is not what this test measures.
    run(&mut world, InputFrame::default(), 0.3);
    let used_before_kg = world.air_used_kg();

    run(&mut world, InputFrame::default(), 1.0);

    // SPEC §7.5 and §12: 48 kN / 600 m/s = 80 kg/s, within 1%.
    let used_kg = world.air_used_kg() - used_before_kg;
    assert!((used_kg - 80.0).abs() < 0.8, "used {used_kg} kg");
}

#[test]
fn thrust_stops_smoothly_when_the_tank_runs_dry() {
    let mut world = new_world();
    lift_into_the_air(&mut world, 200.0);
    world.set_air_kg(20.0);
    world.set_controller_throttles([1.0; NOZZLE_COUNT]);

    run(&mut world, InputFrame::default(), 3.0);

    // Thrust fades linearly over the last 2% of air, so the last few grams drain ever
    // more slowly (an exponential tail) instead of cutting off with a jolt. The tank
    // never goes negative and never uses more air than it had.
    assert!(
        world.air_kg() >= 0.0 && world.air_kg() < 0.001,
        "{} kg left",
        world.air_kg()
    );
    assert!(
        world.air_used_kg() <= 20.0 + 1e-3,
        "used {} kg",
        world.air_used_kg()
    );
}

#[test]
fn the_tank_refills_on_the_ground_but_not_in_the_air() {
    let mut world = new_world();
    settle(&mut world);
    world.set_air_kg(100.0);

    run(&mut world, InputFrame::default(), 2.0);
    let after_ground_kg = world.air_kg();
    lift_into_the_air(&mut world, 200.0);
    run(&mut world, InputFrame::default(), 1.0);
    let after_air_kg = world.air_kg();

    // SPEC §7.5: +refill_kg_s with at least 3 wheels down; the settled car has used no
    // air for longer than refill_delay_s, so refilling runs the whole 2 s.
    let expected_kg = 100.0 + 2.0 * world.mode().refill_kg_s;
    assert!(
        (after_ground_kg - expected_kg).abs() < 0.1,
        "{after_ground_kg} kg"
    );
    assert_eq!(after_air_kg, after_ground_kg);
}

#[test]
fn a_realistic_hop_rises_about_1_7_m_and_costs_about_24_kg() {
    let mut world = new_world();

    let rise_m = hop_apex_rise_m(&mut world);

    // SPEC §12: apex 1.7 m ± 20%; SPEC §7.6: about 24 kg of air.
    assert!((rise_m - 1.7).abs() < 0.34, "rise {rise_m} m");
    let used_kg = world.air_used_kg();
    assert!((used_kg - 24.0).abs() < 4.8, "used {used_kg} kg");
}

#[test]
fn an_arcade_hop_rises_about_7_5_m() {
    let mut world = new_world_in(arcade());

    let rise_m = hop_apex_rise_m(&mut world);

    // SPEC §12: apex 7.5 m ± 20%.
    assert!((rise_m - 7.5).abs() < 1.5, "rise {rise_m} m");
}

#[test]
fn hop_is_refused_without_enough_air_in_realistic_mode() {
    let mut world = new_world();
    settle(&mut world);
    world.set_air_kg(10.0);

    world.step(buttons(BUTTON_HOP));

    assert_eq!(world.hop_blocked_reason(), Some(HopBlockedReason::NoAir));
    assert!(!world.hop_boost().hop_active());
}

#[test]
fn boost_adds_about_0_9_g_in_realistic_and_twice_that_in_arcade() {
    for (mode, expected_g) in [(realistic(), 0.9), (arcade(), 1.8)] {
        let mut world = new_world_in(mode);
        lift_into_the_air(&mut world, 200.0);
        run(&mut world, buttons(BUTTON_BOOST), 0.3);
        let speed_before_m_s = world.forward_speed_m_s();

        run(&mut world, buttons(BUTTON_BOOST), 0.5);

        let acceleration_g = (world.forward_speed_m_s() - speed_before_m_s) / 0.5 / GRAVITY_M_S2;
        assert!(
            (acceleration_g - expected_g).abs() < 0.1,
            "{acceleration_g} g, expected {expected_g} g"
        );
    }
}

#[test]
fn placing_the_car_ends_a_hop_in_progress() {
    let mut world = new_world();
    settle(&mut world);
    world.step(buttons(BUTTON_HOP));
    assert!(world.hop_boost().hop_active());

    lift_into_the_air(&mut world, 50.0);

    assert!(!world.hop_boost().hop_active());
    assert_eq!(world.hop_boost().hop_cooldown_left_s(), 0.0);
}

#[test]
fn holding_hop_starts_only_one_hop_even_after_the_cooldown() {
    let mut world = new_world();
    settle(&mut world);

    // Hold Hop well past the cooldown and count how many Hops begin.
    let mut hops_started = 0;
    let mut was_active = false;
    for _ in 0..4 * STEPS_PER_SECOND {
        world.step(buttons(BUTTON_HOP));
        let active = world.hop_boost().hop_active();
        if active && !was_active {
            hops_started += 1;
        }
        was_active = active;
    }

    assert_eq!(hops_started, 1);
}

#[test]
fn out_of_air_starts_where_thrust_starts_to_fade_and_never_in_arcade() {
    let mut world = new_world();
    let capacity_kg = realistic().tank_capacity_kg;
    let fade_start_kg = realistic().empty_fade_fraction * capacity_kg;

    world.set_air_kg(fade_start_kg * 1.5);
    assert!(!world.is_out_of_air());
    world.set_air_kg(fade_start_kg * 0.5);
    assert!(world.is_out_of_air());

    let mut arcade_world = new_world_in(arcade());
    arcade_world.set_air_kg(0.0);
    assert!(!arcade_world.is_out_of_air());
}

#[test]
fn an_empty_tank_shows_no_flames_even_with_the_valves_open() {
    let mut world = new_world();
    // In the air, where the thruster interlock never cuts the controller.
    lift_into_the_air(&mut world, 20.0);
    world.set_controller_throttles([1.0; NOZZLE_COUNT]);
    run(&mut world, InputFrame::default(), 0.3);
    assert!(world.flame_fractions()[0] > 0.5);

    world.set_air_kg(0.0);

    assert!(
        world.effective_throttles()[0] > 0.5,
        "the valves are still open"
    );
    assert_eq!(world.flame_fractions(), [0.0; NOZZLE_COUNT]);
}
