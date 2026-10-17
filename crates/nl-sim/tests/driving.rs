//! Driving tests for M1: the car settles on its wheels, drives, steers and brakes.
//! Each test builds a fresh world from the real `configs/vehicle.toml`.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::new_world;
use nl_sim::conventions::PHYSICS_HZ;
use nl_sim::input::InputFrame;
use nl_sim::world::World;

const STEPS_PER_SECOND: usize = PHYSICS_HZ as usize;

fn run(world: &mut World, input: InputFrame, seconds: f32) {
    let steps = (seconds * STEPS_PER_SECOND as f32).round() as usize;
    for _ in 0..steps {
        world.step(input);
    }
}

fn throttle(amount: i8) -> InputFrame {
    throttle_and_steer(amount, 0)
}

fn throttle_and_steer(throttle: i8, steer: i8) -> InputFrame {
    InputFrame {
        throttle,
        steer,
        ..InputFrame::default()
    }
}

#[test]
fn car_settles_on_four_wheels_with_its_centre_about_0_6_m_up() {
    let mut world = new_world();

    run(&mut world, InputFrame::default(), 2.0);

    let height_m = world.chassis_pose().position_m[1];
    assert!(
        (height_m - 0.6).abs() < 0.05,
        "centre of mass at {height_m} m"
    );
    assert_eq!(world.wheels_touching(), [true; 4]);
}

#[test]
fn full_throttle_drives_forward_along_world_x() {
    let mut world = new_world();
    run(&mut world, InputFrame::default(), 1.0);

    run(&mut world, throttle(127), 3.0);

    let speed_m_s = world.forward_speed_m_s();
    let position_m = world.chassis_pose().position_m;
    assert!(
        speed_m_s > 20.0,
        "speed after 3 s of full throttle: {speed_m_s} m/s"
    );
    assert!(position_m[0] > 25.0, "x after 3 s: {} m", position_m[0]);
    assert!(
        position_m[2].abs() < 0.5,
        "drifted sideways to z = {} m",
        position_m[2]
    );
}

#[test]
fn speed_stays_under_the_limit() {
    let mut world = new_world();

    run(&mut world, throttle(127), 15.0);

    let speed_m_s = world.forward_speed_m_s();
    assert!(speed_m_s < 46.0, "speed after 15 s: {speed_m_s} m/s");
}

#[test]
fn steering_left_turns_the_nose_left() {
    let mut world = new_world();
    run(&mut world, throttle(127), 2.0);

    run(&mut world, throttle_and_steer(60, 127), 1.0);

    // SPEC §4: positive yaw rate means the nose turns left. Driving along +X,
    // turning left moves the car towards -Z.
    let yaw_rate_rad_s = world.angular_velocity_rad_s()[1];
    assert!(yaw_rate_rad_s > 0.1, "yaw rate {yaw_rate_rad_s} rad/s");
    assert!(world.chassis_pose().position_m[2] < -0.5);
}

#[test]
fn wheels_sit_at_the_corners_below_the_body() {
    let mut world = new_world();
    run(&mut world, InputFrame::default(), 1.0);

    let wheels = world.wheel_poses();

    // Order is FL, FR, RL, RR; front is +X, left is -Z; wheel centres are one radius
    // (0.34 m) above the flat ground at y = 0.
    let [front_left, front_right, rear_left, rear_right] = wheels.map(|pose| pose.position_m);
    assert!(front_left[0] > 1.0 && front_left[2] < -0.5);
    assert!(front_right[0] > 1.0 && front_right[2] > 0.5);
    assert!(rear_left[0] < -1.0 && rear_left[2] < -0.5);
    assert!(rear_right[0] < -1.0 && rear_right[2] > 0.5);
    for wheel in wheels {
        assert!(
            (wheel.position_m[1] - 0.34).abs() < 0.03,
            "wheel y {}",
            wheel.position_m[1]
        );
    }
}

#[test]
fn wheels_move_with_the_body_at_speed() {
    let mut world = new_world();
    run(&mut world, throttle(127), 3.0);

    let body_x_m = world.chassis_pose().position_m[0];
    let front_left_x_m = world.wheel_poses()[0].position_m[0];

    // The front-left mount is 1.3 m ahead of the centre of mass, also at 25+ m/s.
    assert!(
        (front_left_x_m - body_x_m - 1.3).abs() < 0.05,
        "wheel is {} m ahead of the body",
        front_left_x_m - body_x_m
    );
}

#[test]
fn wheels_are_at_their_mounts_before_the_first_step() {
    let world = new_world();

    let body_y_m = world.chassis_pose().position_m[1];
    let wheel_y_m = world.wheel_poses()[0].position_m[1];

    // Not at the origin: below the body by up to the rest length (0.3 m).
    assert!(
        wheel_y_m < body_y_m && wheel_y_m > body_y_m - 0.35,
        "wheel y {wheel_y_m}"
    );
}

#[test]
fn steering_is_rate_limited() {
    let mut world = new_world();

    world.step(throttle_and_steer(0, 127));

    // 2 rad/s × 1/120 s = 0.0167 rad after one step, not the full 0.5 rad.
    let angle_rad = world.steering_angle_rad();
    assert!(
        angle_rad > 0.0 && angle_rad < 0.02,
        "angle after one step: {angle_rad} rad"
    );
}

#[test]
fn braking_from_20_m_s_stops_the_car_in_a_plausible_distance() {
    let mut world = new_world();
    run(&mut world, throttle(127), 2.0);
    let start_x_m = world.chassis_pose().position_m[0];
    let start_speed_m_s = world.forward_speed_m_s();

    // Brake until stopped. Holding the brake longer would start reversing.
    let mut braking_steps = 0;
    while world.forward_speed_m_s() > 0.5 && braking_steps < 10 * STEPS_PER_SECOND {
        world.step(throttle(-127));
        braking_steps += 1;
    }

    // About 1 g of braking: v² / (2 · 9.81) ≈ 20 m from 20 m/s.
    let stopping_distance_m = world.chassis_pose().position_m[0] - start_x_m;
    assert!(
        stopping_distance_m > 15.0 && stopping_distance_m < 40.0,
        "stopped in {stopping_distance_m} m from {start_speed_m_s} m/s"
    );
}

#[test]
fn letting_go_of_the_throttle_coasts_to_a_slow_stop() {
    let mut world = new_world();
    run(&mut world, throttle(127), 2.0);

    run(&mut world, InputFrame::default(), 3.0);

    // Rolling resistance and drag slow the car gently: it neither stops dead nor
    // keeps its speed.
    let speed_m_s = world.forward_speed_m_s();
    assert!(
        speed_m_s > 15.0 && speed_m_s < 21.0,
        "speed after coasting: {speed_m_s} m/s"
    );
}

#[test]
fn pulling_back_from_standstill_reverses() {
    let mut world = new_world();
    run(&mut world, InputFrame::default(), 1.0);

    run(&mut world, throttle(-127), 2.0);

    assert!(world.forward_speed_m_s() < -1.0);
}
