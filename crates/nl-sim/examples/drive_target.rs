//! Measures a candidate `[drive]` / `[wheels]` setup against the fast-roadster target
//! (research R2a, plan 2026-10-27): 0–100 km/h, top speed, wheelspin while launching,
//! steering and lateral grip at 50–100 m/s, and how steady the raycast wheels are at
//! 100 m/s on flat ground, over floor seams and over a shallow ramp's kinks.
//!
//! Run with: `cargo run --release -p nl-sim --example drive_target [-- path/to/vehicle.toml]`
//! The file is a full vehicle TOML (copy `configs/vehicle.toml` and edit the copy); the
//! repo's own config is never changed by this tool.

use std::error::Error;

use nl_sim::conventions::{PHYSICS_DT_S, PHYSICS_HZ};
use nl_sim::course::{BoundsSpec, CourseSpec, PieceSpec, StartSpec};
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

const STANDARD_GRAVITY_M_S2: f32 = 9.81;
/// 100 km/h and 200 km/h in m/s.
const HUNDRED_KM_H_M_S: f32 = 100.0 / 3.6;
const TWO_HUNDRED_KM_H_M_S: f32 = 200.0 / 3.6;
/// Seconds of full throttle used to find the top speed.
const TOP_SPEED_RUN_S: f32 = 90.0;
/// Speeds for the steering tests, in m/s.
const STEER_SPEEDS_M_S: [f32; 3] = [50.0, 75.0, 100.0];
/// Steering inputs tested at each speed, as a fraction of the speed's steering limit.
const STEER_FRACTIONS: [f32; 3] = [0.1, 0.3, 1.0];
/// Each steering test lasts this long; the last `STEER_AVERAGE_S` give the steady values.
const STEER_RUN_S: f32 = 4.0;
const STEER_AVERAGE_S: f32 = 1.0;
/// Speed of the stability runs, in m/s.
const STABILITY_SPEED_M_S: f32 = 100.0;
/// Throttle per m/s of speed error when holding a speed.
const SPEED_HOLD_GAIN_PER_M_S: f32 = 0.5;
/// Floor seams for the seam run: one every this many metres along +X.
const SEAM_SPACING_M: f32 = 7.3;
const SEAM_COUNT: usize = 100;
/// The kink run's shallow ramp: long and low, so a car at 100 m/s stays on it.
const KINK_RAMP_LENGTH_M: f32 = 120.0;
const KINK_RAMP_ANGLE_DEG: f32 = 3.0;
const KINK_RAMP_START_M: f32 = 100.0;

fn main() -> Result<(), Box<dyn Error>> {
    let config = load_vehicle_config()?;
    print_straight_line(&config)?;
    print_steering(&config)?;
    print_stability(&config)?;
    Ok(())
}

/// The TOML file given as the first argument, or the repo's car.
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
    let mut world = World::new_on_flat_ground(config, modes.realistic, DEFAULT_GROUND_FRICTION, 7);
    for _ in 0..PHYSICS_HZ {
        world.step(InputFrame::default());
    }
    Ok(world)
}

fn seconds_to_steps(seconds: f32) -> usize {
    (seconds * PHYSICS_HZ as f32).round() as usize
}

fn drive(throttle: i8, steer: i8) -> InputFrame {
    InputFrame {
        throttle,
        steer,
        ..InputFrame::default()
    }
}

/// Throttle that holds `target_m_s`, braking if too fast.
fn speed_hold_throttle(world: &World, target_m_s: f32) -> i8 {
    let error_m_s = target_m_s - world.forward_speed_m_s();
    let fraction = (error_m_s * SPEED_HOLD_GAIN_PER_M_S).clamp(-1.0, 1.0);
    (fraction * 127.0).round() as i8
}

/// What a full-throttle launch from rest did.
struct Launch {
    to_100_km_h_s: Option<f32>,
    to_200_km_h_s: Option<f32>,
    top_speed_m_s: f32,
    /// Time to get within 1 m/s of the top speed.
    to_top_s: f32,
    /// Smallest share of the asked engine force the tyres delivered, over steps with
    /// all wheels down in the first 2 s (1 = no wheelspin).
    worst_traction_share: f32,
    /// Peak forward acceleration, in g.
    peak_accel_g: f32,
    /// Most nose-up pitch during the launch, in degrees.
    peak_squat_deg: f32,
    /// Smallest front-wheel suspension force during the first 2 s, in N.
    min_front_load_n: f32,
}

fn print_straight_line(config: &VehicleConfig) -> Result<(), Box<dyn Error>> {
    let launch = launch(config)?;
    println!("Straight line (Realistic, flat ground)");
    println!(
        "  0-100 km/h          {}",
        format_time(launch.to_100_km_h_s)
    );
    println!(
        "  0-200 km/h          {}",
        format_time(launch.to_200_km_h_s)
    );
    println!(
        "  top speed           {:6.1} m/s ({:.0} km/h), within 1 m/s after {:.1} s",
        launch.top_speed_m_s,
        launch.top_speed_m_s * 3.6,
        launch.to_top_s
    );
    println!("  peak accel          {:6.2} g", launch.peak_accel_g);
    println!(
        "  traction share      {:6.2} (worst step in the first 2 s; 1 = no wheelspin)",
        launch.worst_traction_share
    );
    println!(
        "  launch squat        {:6.1} deg nose up",
        launch.peak_squat_deg
    );
    println!(
        "  min front load      {:6.0} N per wheel (first 2 s)",
        launch.min_front_load_n
    );
    println!();
    Ok(())
}

/// Full throttle from rest for `TOP_SPEED_RUN_S`, recording the launch numbers.
fn launch(config: &VehicleConfig) -> Result<Launch, Box<dyn Error>> {
    let mut world = settled_world(config)?;
    let mut result = Launch {
        to_100_km_h_s: None,
        to_200_km_h_s: None,
        top_speed_m_s: 0.0,
        to_top_s: 0.0,
        worst_traction_share: 1.0,
        peak_accel_g: 0.0,
        peak_squat_deg: 0.0,
        min_front_load_n: f32::INFINITY,
    };
    let mut speeds_m_s = Vec::new();
    let mut previous_m_s = world.forward_speed_m_s();
    for step in 1..=seconds_to_steps(TOP_SPEED_RUN_S) {
        world.step(drive(127, 0));
        let time_s = step as f32 * PHYSICS_DT_S;
        let speed_m_s = world.forward_speed_m_s();
        speeds_m_s.push(speed_m_s);
        if speed_m_s >= HUNDRED_KM_H_M_S && result.to_100_km_h_s.is_none() {
            result.to_100_km_h_s = Some(time_s);
        }
        if speed_m_s >= TWO_HUNDRED_KM_H_M_S && result.to_200_km_h_s.is_none() {
            result.to_200_km_h_s = Some(time_s);
        }
        let accel_g = (speed_m_s - previous_m_s) / PHYSICS_DT_S / STANDARD_GRAVITY_M_S2;
        previous_m_s = speed_m_s;
        if time_s <= 2.0 {
            result.peak_accel_g = result.peak_accel_g.max(accel_g);
            record_launch_wheels(&world, &mut result);
        }
        result.peak_squat_deg = result.peak_squat_deg.max(pitch_rad(&world).to_degrees());
    }
    result.top_speed_m_s = speeds_m_s.iter().copied().fold(0.0, f32::max);
    let first_near_top = speeds_m_s
        .iter()
        .position(|speed_m_s| *speed_m_s >= result.top_speed_m_s - 1.0)
        .unwrap_or(0);
    result.to_top_s = (first_near_top + 1) as f32 * PHYSICS_DT_S;
    Ok(result)
}

/// Folds this step's wheel loads and delivered drive into the launch record.
fn record_launch_wheels(world: &World, result: &mut Launch) {
    let wheels = world.wheel_diagnostics();
    if !wheels.iter().all(|wheel| wheel.in_contact) {
        return;
    }
    let mut asked_n_s = 0.0;
    let mut delivered_n_s = 0.0;
    for wheel in &wheels {
        asked_n_s += wheel.engine_force_n * PHYSICS_DT_S;
        delivered_n_s += wheel.forward_impulse_n_s;
    }
    if asked_n_s > 0.0 {
        let share = delivered_n_s / asked_n_s;
        result.worst_traction_share = result.worst_traction_share.min(share);
    }
    for wheel in &wheels[..2] {
        result.min_front_load_n = result.min_front_load_n.min(wheel.suspension_force_n);
    }
}

/// Nose-up pitch: the angle of the body's +X above the horizontal, in rad.
fn pitch_rad(world: &World) -> f32 {
    let [x, y, z, w] = world.chassis_pose().rotation_xyzw;
    let forward_y = 2.0 * (x * y + w * z);
    libm::asinf(forward_y.clamp(-1.0, 1.0))
}

/// Roll: the angle of the body's +Z above the horizontal, in rad.
fn roll_rad(world: &World) -> f32 {
    let [x, y, z, w] = world.chassis_pose().rotation_xyzw;
    let right_y = 2.0 * (y * z - w * x);
    libm::asinf(right_y.clamp(-1.0, 1.0))
}

fn format_time(time_s: Option<f32>) -> String {
    match time_s {
        Some(time_s) => format!("{time_s:6.2} s"),
        None => "  never".to_string(),
    }
}

/// What holding one steering input at a held speed did.
struct SteerResult {
    speed_m_s: f32,
    lateral_g: f32,
    radius_m: f32,
    max_slip_deg: f32,
    max_tilt_deg: f32,
    wheels_down_share: f32,
}

fn print_steering(config: &VehicleConfig) -> Result<(), Box<dyn Error>> {
    println!("Steering at a held speed (fraction of the speed's steering limit)");
    println!("  target  steer  speed  lat_g  radius_m  max_slip_deg  max_tilt_deg  wheels_down");
    for target_m_s in STEER_SPEEDS_M_S {
        for fraction in STEER_FRACTIONS {
            let turn = steer_test(config, target_m_s, fraction)?;
            println!(
                "  {:6.0} {:6.2} {:6.1} {:6.2} {:9.0} {:13.1} {:13.1} {:12.2}",
                target_m_s,
                fraction,
                turn.speed_m_s,
                turn.lateral_g,
                turn.radius_m,
                turn.max_slip_deg,
                turn.max_tilt_deg,
                turn.wheels_down_share
            );
        }
    }
    println!();
    Ok(())
}

/// Places the car at `target_m_s` going straight, settles it for 1 s, then holds
/// `fraction` of left lock while holding the speed.
fn steer_test(
    config: &VehicleConfig,
    target_m_s: f32,
    fraction: f32,
) -> Result<SteerResult, Box<dyn Error>> {
    let mut world = settled_world(config)?;
    let start = world.chassis_pose();
    world.place_car(
        start.position_m,
        start.rotation_xyzw,
        [target_m_s, 0.0, 0.0],
        [0.0; 3],
    )?;
    for _ in 0..PHYSICS_HZ {
        world.step(drive(speed_hold_throttle(&world, target_m_s), 0));
    }
    let steer = (fraction * 127.0).round() as i8;
    let steps = seconds_to_steps(STEER_RUN_S);
    let average_from = steps - seconds_to_steps(STEER_AVERAGE_S);
    let mut result = SteerResult {
        speed_m_s: 0.0,
        lateral_g: 0.0,
        radius_m: 0.0,
        max_slip_deg: 0.0,
        max_tilt_deg: 0.0,
        wheels_down_share: 0.0,
    };
    let mut yaw_rate_sum_rad_s = 0.0;
    let mut speed_sum_m_s = 0.0;
    let mut wheels_down_steps = 0;
    for step in 0..steps {
        world.step(drive(speed_hold_throttle(&world, target_m_s), steer));
        let [vx, _, vz] = world.linear_velocity_m_s();
        let slip_deg = slip_rad(&world, vx, vz).abs().to_degrees();
        result.max_slip_deg = result.max_slip_deg.max(slip_deg);
        result.max_tilt_deg = result.max_tilt_deg.max(world.tilt_rad().to_degrees());
        if world.wheels_touching_count() == 4 {
            wheels_down_steps += 1;
        }
        if step >= average_from {
            speed_sum_m_s += vx.hypot(vz);
            yaw_rate_sum_rad_s += world.angular_velocity_rad_s()[1].abs();
        }
    }
    let average_steps = (steps - average_from) as f32;
    result.speed_m_s = speed_sum_m_s / average_steps;
    let yaw_rate_rad_s = yaw_rate_sum_rad_s / average_steps;
    // Steady turn: lateral acceleration = speed × yaw rate; radius = speed / yaw rate.
    result.lateral_g = result.speed_m_s * yaw_rate_rad_s / STANDARD_GRAVITY_M_S2;
    result.radius_m = result.speed_m_s / yaw_rate_rad_s.max(1e-6);
    result.wheels_down_share = wheels_down_steps as f32 / steps as f32;
    Ok(result)
}

/// Angle between where the car is going and where its nose points, in rad.
fn slip_rad(world: &World, vx: f32, vz: f32) -> f32 {
    let [x, y, z, w] = world.chassis_pose().rotation_xyzw;
    let forward_x = 1.0 - 2.0 * (y * y + z * z);
    let forward_z = 2.0 * (x * z - w * y);
    let course_rad = (-vz).atan2(vx);
    let nose_rad = (-forward_z).atan2(forward_x);
    let tau = std::f32::consts::TAU;
    (course_rad - nose_rad + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}

/// How steady the car was during a straight run at a held speed.
struct StabilityResult {
    /// Share of steps with all four wheel rays touching.
    all_wheels_share: f32,
    /// Steps where some wheel lost its ray contact (and how many such dropouts).
    dropout_steps: usize,
    /// Largest |vertical speed|, |pitch| and |roll| during the run.
    max_vertical_m_s: f32,
    max_pitch_deg: f32,
    max_roll_deg: f32,
    /// Largest per-step change of any wheel's compression, in m (jitter).
    max_compression_jump_m: f32,
    /// Steps where the body touched anything, and the crash flag.
    chassis_touch_steps: usize,
    crashed: bool,
}

fn print_stability(config: &VehicleConfig) -> Result<(), Box<dyn Error>> {
    println!("Stability at {STABILITY_SPEED_M_S} m/s for the run (straight, held speed)");
    println!(
        "  surface      all4_share  dropouts  max_vz  max_pitch  max_roll  max_jump_m  body_touch  crashed"
    );
    let cases = [
        ("flat", flat_course(&[])),
        ("seams", flat_course(&seam_pieces())),
        ("3deg_ramp", flat_course(&[kink_ramp()])),
    ];
    for (name, course) in cases {
        let run = stability_run(config, &course)?;
        println!(
            "  {:<12} {:10.3} {:9} {:7.2} {:10.2} {:9.2} {:11.4} {:11} {:8}",
            name,
            run.all_wheels_share,
            run.dropout_steps,
            run.max_vertical_m_s,
            run.max_pitch_deg,
            run.max_roll_deg,
            run.max_compression_jump_m,
            run.chassis_touch_steps,
            run.crashed
        );
    }
    Ok(())
}

/// A long straight course along +X with `pieces` on it.
fn flat_course(pieces: &[PieceSpec]) -> CourseSpec {
    CourseSpec {
        name: "Stability".to_string(),
        start: StartSpec {
            pos: [0.0, 0.7, 0.0],
            yaw: 0.0,
        },
        bounds: BoundsSpec {
            min: [-50.0, -20.0, -50.0],
            max: [2000.0, 80.0, 50.0],
        },
        pieces: pieces.to_vec(),
    }
}

/// Zero-length gaps: they split the floor into separate boxes that meet exactly, so
/// the wheels roll over box seams (the course loader would refuse a zero gap, so the
/// spec is built directly).
fn seam_pieces() -> Vec<PieceSpec> {
    let mut pieces = Vec::new();
    for seam in 1..=SEAM_COUNT {
        pieces.push(PieceSpec::Gap {
            pos: [seam as f32 * SEAM_SPACING_M, 0.0, 0.0],
            length: 0.0,
        });
    }
    pieces
}

/// A long, shallow curved ramp: 6 segments, each 0.5° steeper than the last.
fn kink_ramp() -> PieceSpec {
    PieceSpec::Ramp {
        pos: [KINK_RAMP_START_M, 0.0, 0.0],
        yaw: 0.0,
        length: KINK_RAMP_LENGTH_M,
        width: 10.0,
        angle_deg: KINK_RAMP_ANGLE_DEG,
    }
}

/// Settles the car at the course start, sets it rolling at `STABILITY_SPEED_M_S`, and
/// holds that speed in a straight line until it passes the kink ramp's lip (or 9 s).
fn stability_run(
    config: &VehicleConfig,
    course: &CourseSpec,
) -> Result<StabilityResult, Box<dyn Error>> {
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)?;
    let mut world =
        World::new_on_course(config, modes.realistic, course, DEFAULT_GROUND_FRICTION, 7);
    for _ in 0..PHYSICS_HZ {
        world.step(InputFrame::default());
    }
    let start = world.chassis_pose();
    let speed_m_s = STABILITY_SPEED_M_S;
    world.place_car(
        start.position_m,
        start.rotation_xyzw,
        [speed_m_s, 0.0, 0.0],
        [0.0; 3],
    )?;
    // Let the rays find the ground again before measuring.
    for _ in 0..(PHYSICS_HZ / 4) {
        world.step(drive(speed_hold_throttle(&world, speed_m_s), 0));
    }
    let mut result = StabilityResult {
        all_wheels_share: 0.0,
        dropout_steps: 0,
        max_vertical_m_s: 0.0,
        max_pitch_deg: 0.0,
        max_roll_deg: 0.0,
        max_compression_jump_m: 0.0,
        chassis_touch_steps: 0,
        crashed: false,
    };
    // Stop 3 m short of the lip, so the front wheels leaving it is not counted.
    let lip_x_m = KINK_RAMP_START_M + KINK_RAMP_LENGTH_M - 3.0;
    let mut previous = world.wheel_diagnostics();
    let mut steps = 0;
    let mut all_down_steps = 0;
    while steps < seconds_to_steps(9.0) && world.chassis_pose().position_m[0] < lip_x_m {
        world.step(drive(speed_hold_throttle(&world, speed_m_s), 0));
        steps += 1;
        let wheels = world.wheel_diagnostics();
        if wheels.iter().all(|wheel| wheel.in_contact) {
            all_down_steps += 1;
        } else {
            result.dropout_steps += 1;
        }
        for (now, before) in wheels.iter().zip(previous.iter()) {
            let jump_m = (now.compression_m - before.compression_m).abs();
            result.max_compression_jump_m = result.max_compression_jump_m.max(jump_m);
        }
        previous = wheels;
        let vertical_m_s = world.linear_velocity_m_s()[1].abs();
        result.max_vertical_m_s = result.max_vertical_m_s.max(vertical_m_s);
        result.max_pitch_deg = result
            .max_pitch_deg
            .max(pitch_rad(&world).to_degrees().abs());
        result.max_roll_deg = result.max_roll_deg.max(roll_rad(&world).to_degrees().abs());
        if world.chassis_touching() {
            result.chassis_touch_steps += 1;
        }
        result.crashed |= world.is_crashed();
    }
    result.all_wheels_share = all_down_steps as f32 / steps.max(1) as f32;
    Ok(result)
}
