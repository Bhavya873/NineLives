//! Driving tests for M1: the car settles on its wheels, drives, steers and brakes.
//! Each test builds a fresh world from the real `configs/vehicle.toml`.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, arcade, new_world, vehicle};
use nl_sim::conventions::{PHYSICS_DT_S, PHYSICS_HZ};
use nl_sim::input::{BUTTON_BOOST, InputFrame};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

const STEPS_PER_SECOND: usize = PHYSICS_HZ as usize;

fn run(world: &mut World, input: InputFrame, seconds: f32) {
    let steps = (seconds * STEPS_PER_SECOND as f32).round() as usize;
    for _ in 0..steps {
        world.step(input);
    }
}

/// Full throttle in a straight line until the forward speed reaches `target_m_s`.
fn accelerate_to(world: &mut World, target_m_s: f32) {
    let mut steps = 0;
    while world.forward_speed_m_s() < target_m_s && steps < 30 * STEPS_PER_SECOND {
        world.step(throttle(127));
        steps += 1;
    }
}

/// Full brake until the car is nearly stopped; returns the distance travelled in metres.
/// Holding the brake longer would start reversing.
fn brake_to_a_stop_m(world: &mut World) -> f32 {
    let start_x_m = world.chassis_pose().position_m[0];
    let mut steps = 0;
    while world.forward_speed_m_s() > 0.5 && steps < 10 * STEPS_PER_SECOND {
        world.step(throttle(-127));
        steps += 1;
    }
    world.chassis_pose().position_m[0] - start_x_m
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
        speed_m_s > 15.0,
        "speed after 3 s of full throttle: {speed_m_s} m/s"
    );
    assert!(position_m[0] > 20.0, "x after 3 s: {} m", position_m[0]);
    assert!(
        position_m[2].abs() < 0.5,
        "drifted sideways to z = {} m",
        position_m[2]
    );
}

#[test]
fn speed_stays_under_the_limit() {
    let mut world = new_world();

    run(&mut world, throttle(127), 20.0);

    let speed_m_s = world.forward_speed_m_s();
    let limit_m_s = vehicle().drive.max_speed_m_s;
    assert!(
        speed_m_s < limit_m_s + 1.0,
        "speed after 20 s: {speed_m_s} m/s, limit {limit_m_s} m/s"
    );
}

#[test]
fn zero_to_100_km_h_takes_about_two_seconds() {
    let mut world = new_world();
    run(&mut world, InputFrame::default(), 1.0);
    let start_s = world.time_s();

    accelerate_to(&mut world, 100.0 / 3.6);

    // The fast roadster (research R2a, DECISIONS 2026-10-29): 1.98 s measured.
    let elapsed_s = world.time_s() - start_s;
    assert!(
        (1.7..=2.05).contains(&elapsed_s),
        "0-100 km/h took {elapsed_s} s"
    );
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

    // The front-left mount is 1.3 m ahead of the centre of mass, also at 20 m/s.
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

    // At most max_rate × 1/120 s after one step, not the full low-speed limit.
    let angle_rad = world.steering_angle_rad();
    let one_step_rad = vehicle().steering.max_rate_rad_s * PHYSICS_DT_S;
    assert!(
        angle_rad > 0.0 && angle_rad <= one_step_rad + 1e-6,
        "angle after one step: {angle_rad} rad"
    );
    assert!(angle_rad < vehicle().steering.max_angle_low_speed_rad);
}

#[test]
fn braking_from_20_m_s_stops_the_car_in_a_plausible_distance() {
    let mut world = new_world();
    accelerate_to(&mut world, 20.0);
    let start_speed_m_s = world.forward_speed_m_s();

    let stopping_distance_m = brake_to_a_stop_m(&mut world);

    // About 1 g of braking: v² / (2 · 9.81) ≈ 20 m from 20 m/s.
    assert!(
        stopping_distance_m > 15.0 && stopping_distance_m < 40.0,
        "stopped in {stopping_distance_m} m from {start_speed_m_s} m/s"
    );
}

#[test]
fn braking_from_100_km_h_stops_within_45_m() {
    let mut world = new_world();
    accelerate_to(&mut world, 100.0 / 3.6);

    let stopping_distance_m = brake_to_a_stop_m(&mut world);

    assert!(
        stopping_distance_m < 45.0,
        "stopped in {stopping_distance_m} m from 100 km/h"
    );
}

#[test]
fn full_lock_at_30_m_s_neither_spins_nor_rolls_the_car() {
    let mut world = new_world();
    accelerate_to(&mut world, 30.0);
    let mut max_tilt_rad: f32 = 0.0;
    let mut max_yaw_rate_rad_s: f32 = 0.0;

    // Hold full left lock for 4 s with half throttle.
    for _ in 0..4 * STEPS_PER_SECOND {
        world.step(throttle_and_steer(64, 127));
        max_tilt_rad = max_tilt_rad.max(world.tilt_rad());
        max_yaw_rate_rad_s = max_yaw_rate_rad_s.max(world.angular_velocity_rad_s()[1].abs());
    }

    // A grip-limited circle at ~30 m/s and ~1.3 g turns at about 0.4 rad/s; a spin
    // shows up as a much faster yaw rate, a roll as a large tilt.
    assert!(
        max_tilt_rad < 30f32.to_radians(),
        "tilted {max_tilt_rad} rad"
    );
    assert!(
        max_yaw_rate_rad_s < 1.0,
        "yaw rate reached {max_yaw_rate_rad_s} rad/s"
    );
    assert!(world.wheels_touching_count() == 4);
}

#[test]
fn letting_go_of_the_throttle_coasts_to_a_slow_stop() {
    let mut world = new_world();
    accelerate_to(&mut world, 20.0);

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

/// Average forward acceleration over one second of full throttle from `from_m_s`, in m/s².
fn full_throttle_acceleration_m_s2(world: &mut World, from_m_s: f32) -> f32 {
    accelerate_to(world, from_m_s);
    let start_m_s = world.forward_speed_m_s();
    run(world, throttle(127), 1.0);
    world.forward_speed_m_s() - start_m_s
}

#[test]
fn above_the_power_limit_acceleration_falls_with_speed() {
    let mut slow_world = new_world();
    let mut fast_world = new_world();

    let slow_m_s2 = full_throttle_acceleration_m_s2(&mut slow_world, 5.0);
    let fast_m_s2 = full_throttle_acceleration_m_s2(&mut fast_world, 80.0);

    // SPEC §7.3: the engine gives at most `max_engine_power_w`, so its push is power / speed
    // once that is below the force limit (above 28 m/s for 800 kW); at 80 m/s it is well under
    // half the launch push.
    assert!(slow_m_s2 > 5.0, "launch acceleration {slow_m_s2} m/s²");
    assert!(
        fast_m_s2 < 0.5 * slow_m_s2,
        "acceleration at 80 m/s {fast_m_s2} m/s², at 5 m/s {slow_m_s2} m/s²"
    );
}

#[test]
fn boost_pushes_the_car_forward_at_low_speed() {
    let mut plain_world = new_world();
    let mut boost_world = new_world();
    let mut boost_input = throttle(127);
    boost_input.buttons = BUTTON_BOOST;

    run(&mut plain_world, throttle(127), 2.0);
    run(&mut boost_world, boost_input, 2.0);

    assert!(boost_world.forward_speed_m_s() > plain_world.forward_speed_m_s() + 3.0);
}

#[test]
fn boost_cannot_push_the_car_past_the_boost_speed_limit() {
    let vehicle = vehicle();
    let mut world = World::new_on_flat_ground(&vehicle, arcade(), DEFAULT_GROUND_FRICTION, 0);
    // The flat ground is 4 km long: start at its far end so 24 s at 100 m/s stays on it.
    world
        .place_car([-1950.0, 0.62, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
    let mut boost_input = throttle(127);
    boost_input.buttons = BUTTON_BOOST;

    let mut fastest_m_s: f32 = 0.0;
    for _ in 0..24 * STEPS_PER_SECOND {
        world.step(boost_input);
        fastest_m_s = fastest_m_s.max(world.forward_speed_m_s());
    }

    // Arcade has double thrust and unlimited air: without the fade it passed 240 m/s with
    // the old 55 m/s car. Measured with the roadster drive: 108.7 m/s (391 km/h), above the
    // 103 m/s cap (so Boost does something) and under the 110 m/s boost limit.
    let cap_m_s = vehicle.drive.max_speed_m_s;
    let limit_m_s = vehicle.drive.boost_max_speed_m_s;
    assert!(
        fastest_m_s > cap_m_s + 2.0,
        "Boost did nothing: {fastest_m_s} m/s"
    );
    assert!(
        fastest_m_s < limit_m_s,
        "fastest speed in 24 s of Boost: {fastest_m_s} m/s, limit {limit_m_s} m/s"
    );
}

#[test]
fn full_lock_at_20_m_s_corners_at_road_tyre_grip() {
    let mut world = new_world();
    accelerate_to(&mut world, 20.0);
    let mut peak_lateral_g: f32 = 0.0;

    // Hold full left lock with light throttle; lateral acceleration = yaw rate × speed.
    for _ in 0..3 * STEPS_PER_SECOND {
        world.step(throttle_and_steer(32, 127));
        let yaw_rate_rad_s = world.angular_velocity_rad_s()[1].abs();
        let lateral_g = yaw_rate_rad_s * world.forward_speed_m_s().abs() / 9.81;
        peak_lateral_g = peak_lateral_g.max(lateral_g);
    }

    // Grip 1.9 (R2a, for the 1.6 g launch) holds about 1.57 g here; grip 1.3 held 1.14 g.
    assert!(
        (1.4..1.7).contains(&peak_lateral_g),
        "peak lateral {peak_lateral_g} g"
    );
}
