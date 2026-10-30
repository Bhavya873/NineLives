//! The landing forecast (plan 2026-10-27 G4): touchdown time, point, normal, usable
//! length and the speed/pitch uncertainty.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, realistic, vehicle};
use nl_sim::course::{BoundsSpec, CourseSpec, PieceSpec, StartSpec};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

fn flat_world() -> World {
    World::new_on_flat_ground(&vehicle(), realistic(), DEFAULT_GROUND_FRICTION, 1)
}

fn wall_world() -> World {
    let course = CourseSpec {
        name: "forecast rig".to_string(),
        start: StartSpec {
            pos: [0.0, 0.7, 0.0],
            yaw: 0.0,
        },
        bounds: BoundsSpec {
            min: [-50.0, -20.0, -300.0],
            max: [800.0, 100.0, 300.0],
        },
        pieces: vec![PieceSpec::Wall {
            pos: [40.0, 0.0, 0.0],
            yaw: 0.0,
            height: 2.0,
            with_ramp: false,
        }],
    };
    World::new_on_course(&vehicle(), realistic(), &course, DEFAULT_GROUND_FRICTION, 1)
}

fn launch(world: &mut World, position: [f32; 3], velocity: [f32; 3]) {
    world
        .place_car(position, LEVEL, velocity, [0.0; 3])
        .unwrap();
}

#[test]
fn flat_ground_matches_the_ballistic_answer() {
    let mut world = flat_world();
    launch(&mut world, [0.0, 20.0, 0.0], [20.0, 0.0, 0.0]);
    let forecast = world.forecast_landing().unwrap();
    let g = 9.81_f32;
    let expected_t = (2.0 * (20.0 - 0.6) / g).sqrt();
    assert!(
        (forecast.time_s - expected_t).abs() < 0.05,
        "time {} vs {}",
        forecast.time_s,
        expected_t
    );
    assert!((forecast.point_m[0] - 20.0 * expected_t).abs() < 1.0);
    assert!(forecast.point_m[1].abs() < 0.01);
    assert!(forecast.normal[1] > 0.999);
    assert!(forecast.normal_tilt_deg < 1.0);
    assert!(
        forecast.usable_length_m >= 30.0,
        "{}",
        forecast.usable_length_m
    );
}

#[test]
fn uncertainty_comes_from_speed_and_pitch_samples() {
    let mut world = flat_world();
    launch(&mut world, [0.0, 20.0, 0.0], [20.0, 0.0, 0.0]);
    let slow = world.forecast_landing().unwrap();
    // +-2% of 20 m/s over about 2 s moves the point by about 0.8 m; +-1 degree of pitch
    // gives 0.35 m/s of vertical speed, about 0.03 s of flight time (and 0.7 m more range).
    assert!(
        (1.0..2.0).contains(&slow.position_uncertainty_m),
        "{}",
        slow.position_uncertainty_m
    );
    assert!(
        (0.01..0.08).contains(&slow.time_uncertainty_s),
        "{}",
        slow.time_uncertainty_s
    );
    // A faster car lands farther out, so its position spread is larger.
    launch(&mut world, [0.0, 20.0, 0.0], [40.0, 0.0, 0.0]);
    let fast = world.forecast_landing().unwrap();
    assert!(fast.position_uncertainty_m > slow.position_uncertainty_m);
}

#[test]
fn a_landing_on_an_obstacle_has_no_usable_length() {
    let mut world = wall_world();
    launch(&mut world, [40.5, 12.0, 0.0], [0.0; 3]);
    let forecast = world.forecast_landing().unwrap();
    assert!(
        (forecast.point_m[1] - 2.0).abs() < 0.05,
        "{:?}",
        forecast.point_m
    );
    assert_eq!(forecast.usable_length_m, 0.0);
}

#[test]
fn a_wall_close_ahead_shortens_the_usable_length() {
    let mut world = wall_world();
    launch(&mut world, [35.0, 12.0, 0.0], [1.0, 0.0, 0.0]);
    let forecast = world.forecast_landing().unwrap();
    assert!(
        (8.0..16.0).contains(&forecast.usable_length_m),
        "{}",
        forecast.usable_length_m
    );
}

#[test]
fn nothing_below_gives_no_forecast() {
    let mut world = flat_world();
    // Flying away from the 2 km ground box.
    launch(&mut world, [0.0, 20.0, 0.0], [0.0, 80.0, 0.0]);
    let settings = nl_sim::forecast::ForecastSettings {
        max_time_s: 3.0,
        ..Default::default()
    };
    assert!(world.forecast_landing_with(&settings).is_none());
}
