//! The fast roadster drive (research R2a, plan G2): the repo car reaches 103 m/s at the top
//! (0-100 km/h is pinned in `driving.rs`), stays steady at 100 m/s, and cannot tunnel through a wall.
//!
//! Wheel-ray contact and chassis CCD are tested separately: the wheel tests run with CCD
//! off and on (the wheels are raycasts, so they must not depend on it), and the wall test
//! shows what CCD adds.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, realistic, vehicle};
use nl_sim::conventions::PHYSICS_HZ;
use nl_sim::course::{BoundsSpec, CourseSpec, PieceSpec, StartSpec};
use nl_sim::input::InputFrame;
use nl_sim::vehicle_config::VehicleConfig;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

const HZ: usize = PHYSICS_HZ as usize;
const SEAM_SPACING_M: f32 = 7.3;
const SEAM_COUNT: usize = 100;

fn throttle(amount: i8) -> InputFrame {
    InputFrame {
        throttle: amount,
        ..InputFrame::default()
    }
}

fn hold_speed(world: &World, target_m_s: f32) -> InputFrame {
    let fraction = ((target_m_s - world.forward_speed_m_s()) * 0.5).clamp(-1.0, 1.0);
    throttle((fraction * 127.0).round() as i8)
}

fn config_with_ccd(ccd_enabled: bool) -> VehicleConfig {
    let mut config = vehicle();
    config.chassis.ccd_enabled = ccd_enabled;
    config
}

fn course(pieces: Vec<PieceSpec>) -> CourseSpec {
    CourseSpec {
        name: "Drive".to_string(),
        start: StartSpec {
            pos: [0.0, 0.7, 0.0],
            yaw: 0.0,
        },
        bounds: BoundsSpec {
            min: [-50.0, -20.0, -50.0],
            max: [2000.0, 80.0, 50.0],
        },
        pieces,
    }
}

fn settled(config: &VehicleConfig, course: &CourseSpec) -> World {
    let mut world = World::new_on_course(config, realistic(), course, DEFAULT_GROUND_FRICTION, 7);
    for _ in 0..HZ {
        world.step(InputFrame::default());
    }
    world
}

#[test]
fn repo_car_tops_out_at_103_m_s() {
    let mut world = World::new_on_flat_ground(&vehicle(), realistic(), DEFAULT_GROUND_FRICTION, 7);
    for _ in 0..HZ {
        world.step(InputFrame::default());
    }
    // The flat ground is 4 km long: 20 s at the cap is about 1.7 km, still on it.
    for _ in 0..18 * HZ {
        world.step(throttle(127));
    }
    let top = world.forward_speed_m_s();
    assert!((102.0..=104.0).contains(&top), "top speed {top} m/s");
    // Still at the cap after a long run: the limiter holds, nothing runs away.
    for _ in 0..2 * HZ {
        world.step(throttle(127));
    }
    let later = world.forward_speed_m_s();
    assert!((102.0..=104.0).contains(&later), "later speed {later} m/s");
}

/// All four wheel rays touch the ground on every step, with no dropout.
fn assert_wheels_hold_at_100_m_s(config: &VehicleConfig, pieces: Vec<PieceSpec>, stop_x_m: f32) {
    let course = course(pieces);
    let mut world = settled(config, &course);
    let start = world.chassis_pose();
    world
        .place_car(
            start.position_m,
            start.rotation_xyzw,
            [100.0, 0.0, 0.0],
            [0.0; 3],
        )
        .unwrap();
    for _ in 0..HZ / 4 {
        world.step(hold_speed(&world, 100.0));
    }
    let mut steps = 0;
    while world.chassis_pose().position_m[0] < stop_x_m && steps < 9 * HZ {
        world.step(hold_speed(&world, 100.0));
        steps += 1;
        assert!(
            world
                .wheel_diagnostics()
                .iter()
                .all(|wheel| wheel.in_contact),
            "a wheel ray lost the ground at x = {} m (ccd {})",
            world.chassis_pose().position_m[0],
            config.chassis.ccd_enabled
        );
        assert!(!world.chassis_touching(), "the body touched at 100 m/s");
        assert!(!world.is_crashed());
    }
    assert!(
        world.chassis_pose().position_m[0] >= stop_x_m,
        "the car never got to x = {stop_x_m} m"
    );
}

fn seam_pieces() -> Vec<PieceSpec> {
    (1..=SEAM_COUNT)
        .map(|seam| PieceSpec::Gap {
            pos: [seam as f32 * SEAM_SPACING_M, 0.0, 0.0],
            length: 0.0,
            pit_depth: 0.0,
            entry_height: 0.0,
        })
        .collect()
}

fn kink_ramp() -> PieceSpec {
    PieceSpec::Ramp {
        pos: [100.0, 0.0, 0.0],
        yaw: 0.0,
        length: 120.0,
        width: 10.0,
        angle_deg: 3.0,
        v_complete_min_m_s: 0.0,
    }
}

#[test]
fn wheel_rays_hold_at_100_m_s_on_flat_ground_with_ccd_off_and_on() {
    for ccd in [false, true] {
        assert_wheels_hold_at_100_m_s(&config_with_ccd(ccd), vec![], 500.0);
    }
}

#[test]
fn wheel_rays_hold_at_100_m_s_over_floor_seams_with_ccd_off_and_on() {
    for ccd in [false, true] {
        assert_wheels_hold_at_100_m_s(&config_with_ccd(ccd), seam_pieces(), 600.0);
    }
}

#[test]
fn wheel_rays_hold_at_100_m_s_over_ramp_kinks_with_ccd_off_and_on() {
    for ccd in [false, true] {
        assert_wheels_hold_at_100_m_s(&config_with_ccd(ccd), vec![kink_ramp()], 217.0);
    }
}

#[test]
fn ccd_makes_no_difference_to_a_drive_that_touches_nothing() {
    let mut ends = Vec::new();
    for ccd in [false, true] {
        let mut world = settled(&config_with_ccd(ccd), &course(vec![]));
        for _ in 0..5 * HZ {
            world.step(throttle(127));
        }
        ends.push(world.chassis_pose().position_m);
    }
    assert_eq!(ends[0], ends[1]);
}

/// Drives a 100 m/s car straight at a tall wall and returns the furthest x the chassis
/// centre reached.
fn furthest_x_into_wall_m(ccd_enabled: bool, wall_x_m: f32) -> f32 {
    let course = course(vec![PieceSpec::Wall {
        pos: [wall_x_m, 0.0, 0.0],
        yaw: 0.0,
        height: 3.0,
        with_ramp: false,
    }]);
    let mut world = settled(&config_with_ccd(ccd_enabled), &course);
    let start = world.chassis_pose();
    world
        .place_car(start.position_m, LEVEL, [100.0, 0.0, 0.0], [0.0; 3])
        .unwrap();
    let mut furthest = f32::MIN;
    for _ in 0..3 * HZ {
        world.step(InputFrame::default());
        furthest = furthest.max(world.chassis_pose().position_m[0]);
    }
    furthest
}

#[test]
fn a_car_at_100_m_s_does_not_tunnel_through_a_wall_with_ccd() {
    let wall_x_m = 60.0;
    // The wall is 1 m thick; the car centre must stay in front of its back face.
    let furthest = furthest_x_into_wall_m(true, wall_x_m);
    assert!(
        furthest < wall_x_m + 1.0,
        "the car got through the wall: x = {furthest} m"
    );
}
