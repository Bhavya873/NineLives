//! Passenger g-load (SPEC §9.4): about 1 g sitting on the wheels, about 0 in free fall,
//! and well above 1 g while the lift nozzles brake a fall.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, new_world};
use nl_sim::conventions::NOZZLE_COUNT;
use nl_sim::input::InputFrame;
use nl_sim::world::World;

fn run(world: &mut World, throttles: [f32; NOZZLE_COUNT], steps: usize) {
    for _ in 0..steps {
        world.set_controller_throttles(throttles);
        world.step(InputFrame::default());
    }
}

#[test]
fn a_parked_car_feels_one_g() {
    let mut world = new_world();

    run(&mut world, [0.0; NOZZLE_COUNT], 240);

    assert!((world.g_load() - 1.0).abs() < 0.05, "{} g", world.g_load());
}

#[test]
fn a_falling_car_feels_almost_nothing_and_braking_feels_heavy() {
    let mut world = new_world();
    world
        .place_car([0.0, 40.0, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();

    run(&mut world, [0.0; NOZZLE_COUNT], 60);
    let falling_g = world.g_load();
    let mut lift = [0.0; NOZZLE_COUNT];
    lift[..4].fill(1.0);
    run(&mut world, lift, 60);
    let braking_g = world.g_load();

    assert!(falling_g < 0.1, "falling {falling_g} g");
    assert!(braking_g > 1.5, "braking {braking_g} g");
}

#[test]
fn a_drop_reports_a_raw_peak_above_the_displayed_one_and_a_respawn_clears_it() {
    let mut world = new_world();
    world
        .place_car([0.0, 3.0, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
    assert_eq!(world.peak_g_load(), 0.0);

    // 3 m of fall, the touchdown, and the 0.5 s after it. The stiff bump stop can rebound the
    // car off the ground, and that take-off opens a fresh window (docs/contracts/g-meter.md),
    // so the landing's peak is read tick by tick, not once at the end.
    let mut raw_peak = 0.0_f32;
    let mut displayed_peak = 0.0_f32;
    for _ in 0..480 {
        run(&mut world, [0.0; NOZZLE_COUNT], 1);
        raw_peak = raw_peak.max(world.peak_g_load());
        displayed_peak = displayed_peak.max(world.peak_displayed_g_load());
    }

    // The landing is in the peak; the 0.1 s filter flattens its one-tick spike.
    assert!(raw_peak > 1.5, "raw peak {raw_peak} g");
    assert!(
        raw_peak > displayed_peak,
        "{raw_peak} vs {displayed_peak} g"
    );
    assert!(displayed_peak >= world.g_load());

    world.respawn();
    assert_eq!(world.peak_g_load(), 0.0);
}
