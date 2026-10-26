//! Measures how the car drives on flat ground: acceleration, top speed (also with Boost
//! held), braking and full-lock turns at 10, 20 and 30 m/s. Use it when tuning `[drive]`, `[steering]`
//! and `[wheels]` in `configs/vehicle.toml`.
//!
//! Run with: `cargo run --release -p nl-sim --example drive_feel [-- path/to/vehicle.toml]`

use std::error::Error;

use nl_sim::conventions::PHYSICS_HZ;
use nl_sim::input::{BUTTON_BOOST, InputFrame};
use nl_sim::modes::{DEFAULT_MODES_TOML, ModeParams, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

const STANDARD_GRAVITY_M_S2: f32 = 9.81;
/// 100 km/h in m/s.
const HUNDRED_KM_H_M_S: f32 = 100.0 / 3.6;
/// Below this forward speed the car counts as stopped.
const STOPPED_M_S: f32 = 0.5;
/// Seconds of full throttle used to find the top speed.
const TOP_SPEED_RUN_S: f32 = 40.0;
/// Seconds of full lock in each turn test; the last `TURN_AVERAGE_S` give the radius.
const TURN_RUN_S: f32 = 8.0;
const TURN_AVERAGE_S: f32 = 2.0;
/// Speeds at which full lock is tested, in m/s.
const TURN_SPEEDS_M_S: [f32; 3] = [10.0, 20.0, 30.0];
/// Throttle per m/s of speed error when holding a speed.
const SPEED_HOLD_GAIN_PER_M_S: f32 = 0.5;
/// The car "spins" if its nose points this far from where it is going.
const SPIN_SLIP_DEG: f32 = 45.0;
/// The car "rolls" if it tilts past this.
const ROLL_TILT_DEG: f32 = 30.0;
/// Lateral acceleration is averaged over this many steps to smooth out step noise.
const LATERAL_WINDOW_STEPS: usize = 12;

/// What one full-lock turn at a held speed did.
struct TurnResult {
    speed_m_s: f32,
    radius_m: f32,
    /// Time from turning the wheel until the yaw rate reaches 90 % of its steady value.
    yaw_response_s: f32,
    peak_lateral_g: f32,
    max_slip_deg: f32,
    max_tilt_deg: f32,
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = load_vehicle_config()?;

    println!("Straight line");
    println!(
        "  0-100 km/h        {}",
        format_time(time_to_100_km_h_s(&config))
    );
    println!(
        "  speed after 5 s   {:6.1} m/s",
        speed_after_s(&config, 5.0)
    );
    println!("  top speed         {:6.1} m/s", top_speed_m_s(&config));
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)?;
    println!(
        "  top with Boost    {:6.1} m/s Realistic, {:6.1} m/s Arcade",
        top_speed_with_boost_m_s(&config, modes.realistic),
        top_speed_with_boost_m_s(&config, modes.arcade)
    );
    println!(
        "  brake 100-0 km/h  {:6.1} m",
        braking_distance_from_100_m(&config)
    );
    println!();
    println!("Full lock at a held speed");
    println!(
        "  target  speed  radius  yaw_resp_s  peak_lat_g  max_slip_deg  max_tilt_deg  verdict"
    );
    for target_m_s in TURN_SPEEDS_M_S {
        let turn = full_lock_turn(&config, target_m_s);
        println!(
            "  {:6.0} {:6.1} {:7.1} {:11.2} {:11.2} {:13.1} {:13.1}  {}",
            target_m_s,
            turn.speed_m_s,
            turn.radius_m,
            turn.yaw_response_s,
            turn.peak_lateral_g,
            turn.max_slip_deg,
            turn.max_tilt_deg,
            verdict(&turn)
        );
    }
    Ok(())
}

/// The repo's car, or the TOML file given as the first argument.
fn load_vehicle_config() -> Result<VehicleConfig, Box<dyn Error>> {
    match std::env::args().nth(1) {
        Some(path) => Ok(VehicleConfig::from_toml_str(&std::fs::read_to_string(
            path,
        )?)?),
        None => Ok(VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML)?),
    }
}

/// A Realistic-mode car on flat ground, settled on its wheels for one second.
fn settled_world(config: &VehicleConfig) -> Result<World, Box<dyn Error>> {
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)?;
    Ok(settled_world_in(config, modes.realistic))
}

/// A car in `mode` on flat ground, settled on its wheels for one second.
fn settled_world_in(config: &VehicleConfig, mode: ModeParams) -> World {
    let seed = 1_000_000;
    let mut world = World::new_on_flat_ground(config, mode, DEFAULT_GROUND_FRICTION, seed);
    run(&mut world, InputFrame::default(), 1.0);
    world
}

/// Highest forward speed during a long full-throttle run with Boost held, in m/s.
fn top_speed_with_boost_m_s(config: &VehicleConfig, mode: ModeParams) -> f32 {
    let mut world = settled_world_in(config, mode);
    let mut input = drive(127, 0);
    input.buttons = BUTTON_BOOST;
    let mut top_m_s: f32 = 0.0;
    for _ in 0..seconds_to_steps(TOP_SPEED_RUN_S) {
        world.step(input);
        top_m_s = top_m_s.max(world.forward_speed_m_s());
    }
    top_m_s
}

fn seconds_to_steps(seconds: f32) -> usize {
    (seconds * PHYSICS_HZ as f32).round() as usize
}

fn run(world: &mut World, input: InputFrame, seconds: f32) {
    for _ in 0..seconds_to_steps(seconds) {
        world.step(input);
    }
}

fn drive(throttle: i8, steer: i8) -> InputFrame {
    InputFrame {
        throttle,
        steer,
        ..InputFrame::default()
    }
}

/// Seconds of full throttle from rest to 100 km/h, or None if it never gets there.
fn time_to_100_km_h_s(config: &VehicleConfig) -> Option<f32> {
    let mut world = settled_world(config).ok()?;
    for step in 1..=seconds_to_steps(TOP_SPEED_RUN_S) {
        world.step(drive(127, 0));
        if world.forward_speed_m_s() >= HUNDRED_KM_H_M_S {
            return Some(step as f32 / PHYSICS_HZ as f32);
        }
    }
    None
}

fn format_time(time_s: Option<f32>) -> String {
    match time_s {
        Some(time_s) => format!("{time_s:6.2} s"),
        None => "  never".to_string(),
    }
}

/// Forward speed after `seconds` of full throttle from rest, in m/s.
fn speed_after_s(config: &VehicleConfig, seconds: f32) -> f32 {
    let Ok(mut world) = settled_world(config) else {
        return f32::NAN;
    };
    run(&mut world, drive(127, 0), seconds);
    world.forward_speed_m_s()
}

/// Highest forward speed during a long full-throttle run, in m/s.
fn top_speed_m_s(config: &VehicleConfig) -> f32 {
    let Ok(mut world) = settled_world(config) else {
        return f32::NAN;
    };
    let mut top_m_s: f32 = 0.0;
    for _ in 0..seconds_to_steps(TOP_SPEED_RUN_S) {
        world.step(drive(127, 0));
        top_m_s = top_m_s.max(world.forward_speed_m_s());
    }
    top_m_s
}

/// Distance travelled while braking at full pedal from 100 km/h to a stop, in metres.
fn braking_distance_from_100_m(config: &VehicleConfig) -> f32 {
    let Ok(mut world) = settled_world(config) else {
        return f32::NAN;
    };
    accelerate_to(&mut world, HUNDRED_KM_H_M_S);
    let start_m = world.chassis_pose().position_m;
    let max_steps = seconds_to_steps(TOP_SPEED_RUN_S);
    let mut steps = 0;
    while world.forward_speed_m_s() > STOPPED_M_S && steps < max_steps {
        world.step(drive(-127, 0));
        steps += 1;
    }
    horizontal_distance_m(start_m, world.chassis_pose().position_m)
}

/// Full throttle in a straight line until the forward speed reaches `target_m_s`.
fn accelerate_to(world: &mut World, target_m_s: f32) {
    let max_steps = seconds_to_steps(TOP_SPEED_RUN_S);
    let mut steps = 0;
    while world.forward_speed_m_s() < target_m_s && steps < max_steps {
        world.step(drive(127, 0));
        steps += 1;
    }
}

fn horizontal_distance_m(from_m: [f32; 3], to_m: [f32; 3]) -> f32 {
    (to_m[0] - from_m[0]).hypot(to_m[2] - from_m[2])
}

/// Throttle that holds `target_m_s`, braking if too fast.
fn speed_hold_throttle(world: &World, target_m_s: f32) -> i8 {
    let error_m_s = target_m_s - world.forward_speed_m_s();
    let fraction = (error_m_s * SPEED_HOLD_GAIN_PER_M_S).clamp(-1.0, 1.0);
    (fraction * 127.0).round() as i8
}

/// Reaches `target_m_s` in a straight line, then holds full left lock while trying to
/// keep that speed, and records the radius, lateral g, sideslip and tilt.
fn full_lock_turn(config: &VehicleConfig, target_m_s: f32) -> TurnResult {
    let Ok(mut world) = settled_world(config) else {
        return TurnResult::unmeasured();
    };
    accelerate_to(&mut world, target_m_s);

    let turn_steps = seconds_to_steps(TURN_RUN_S);
    let average_from_step = turn_steps - seconds_to_steps(TURN_AVERAGE_S);
    let mut result = TurnResult::unmeasured();
    let mut speed_sum_m_s = 0.0;
    let mut yaw_rate_sum_rad_s = 0.0;
    let mut course_history_rad = Vec::with_capacity(turn_steps);
    let mut speed_history_m_s = Vec::with_capacity(turn_steps);
    let mut yaw_rate_history_rad_s = Vec::with_capacity(turn_steps);
    for step in 0..turn_steps {
        world.step(drive(speed_hold_throttle(&world, target_m_s), 127));
        let velocity_m_s = world.linear_velocity_m_s();
        let speed_m_s = velocity_m_s[0].hypot(velocity_m_s[2]);
        let course_rad = heading_rad(velocity_m_s[0], velocity_m_s[2]);
        let slip_deg = wrap_angle_rad(course_rad - nose_heading_rad(&world))
            .abs()
            .to_degrees();
        result.max_slip_deg = result.max_slip_deg.max(slip_deg);
        result.max_tilt_deg = result.max_tilt_deg.max(world.tilt_rad().to_degrees());
        course_history_rad.push(course_rad);
        speed_history_m_s.push(speed_m_s);
        yaw_rate_history_rad_s.push(world.angular_velocity_rad_s()[1].abs());
        if step >= LATERAL_WINDOW_STEPS {
            let lateral_g = windowed_lateral_g(&course_history_rad, &speed_history_m_s, step);
            result.peak_lateral_g = result.peak_lateral_g.max(lateral_g);
        }
        if step >= average_from_step {
            speed_sum_m_s += speed_m_s;
            yaw_rate_sum_rad_s += world.angular_velocity_rad_s()[1].abs();
        }
    }
    let average_steps = (turn_steps - average_from_step) as f32;
    result.speed_m_s = speed_sum_m_s / average_steps;
    // Steady circle: radius = speed / yaw rate.
    result.radius_m = speed_sum_m_s / yaw_rate_sum_rad_s.max(1e-6);
    let steady_yaw_rate_rad_s = yaw_rate_sum_rad_s / average_steps;
    result.yaw_response_s = yaw_response_s(&yaw_rate_history_rad_s, steady_yaw_rate_rad_s);
    result
}

/// Seconds until the yaw rate first reaches 90 % of `steady_rad_s`.
fn yaw_response_s(yaw_rate_rad_s: &[f32], steady_rad_s: f32) -> f32 {
    for (step, rate_rad_s) in yaw_rate_rad_s.iter().enumerate() {
        if *rate_rad_s >= 0.9 * steady_rad_s {
            return (step + 1) as f32 / PHYSICS_HZ as f32;
        }
    }
    f32::NAN
}

/// Centripetal acceleration over the last `LATERAL_WINDOW_STEPS`, in g:
/// speed × rate of change of the direction of travel.
fn windowed_lateral_g(course_rad: &[f32], speed_m_s: &[f32], step: usize) -> f32 {
    let start = step - LATERAL_WINDOW_STEPS;
    let window_s = LATERAL_WINDOW_STEPS as f32 / PHYSICS_HZ as f32;
    let course_rate_rad_s = wrap_angle_rad(course_rad[step] - course_rad[start]) / window_s;
    let mean_speed_m_s = 0.5 * (speed_m_s[step] + speed_m_s[start]);
    (mean_speed_m_s * course_rate_rad_s).abs() / STANDARD_GRAVITY_M_S2
}

/// Direction of a horizontal vector (x, z) as a yaw angle: 0 along +X, positive
/// towards -Z, so it grows when the car turns left (SPEC §4).
fn heading_rad(x: f32, z: f32) -> f32 {
    (-z).atan2(x)
}

/// Yaw angle of the car's nose (body +X) in the world.
fn nose_heading_rad(world: &World) -> f32 {
    let [x, y, z, w] = world.chassis_pose().rotation_xyzw;
    // Body +X rotated by the quaternion (first column of its rotation matrix).
    let forward_x = 1.0 - 2.0 * (y * y + z * z);
    let forward_z = 2.0 * (x * z - w * y);
    heading_rad(forward_x, forward_z)
}

/// Wraps an angle into -π..π.
fn wrap_angle_rad(angle_rad: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (angle_rad + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}

fn verdict(turn: &TurnResult) -> &'static str {
    if turn.max_tilt_deg > ROLL_TILT_DEG {
        "ROLLS"
    } else if turn.max_slip_deg > SPIN_SLIP_DEG {
        "SPINS"
    } else {
        "ok"
    }
}

impl TurnResult {
    /// Values before any step is measured; NaN marks a number not yet known.
    fn unmeasured() -> TurnResult {
        TurnResult {
            speed_m_s: f32::NAN,
            radius_m: f32::NAN,
            yaw_response_s: f32::NAN,
            peak_lateral_g: 0.0,
            max_slip_deg: 0.0,
            max_tilt_deg: 0.0,
        }
    }
}
