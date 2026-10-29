//! Measures how much load the suspension can carry before the body touches the ground
//! (research R2b, plan 2026-10-27 §1.4 `n_max`), and how a flat drop lands (the bump-stop
//! energy check). No controller fires: this is the bare car.
//!
//! Curve rig: a smooth concave curve (a corkscrew with a 1° bank, so its centre line is a
//! parabola of radius R = L / tan(angle) at the foot, sampled at 16 steps, kinks under
//! 2.5°) driven straight at held speeds. The design load at the foot is
//! n = v²/R + g; the lowest speed at which the body touches gives the measured n_max.
//!
//! Run with: `cargo run --release -p nl-sim --example suspension_load [-- path/to/vehicle.toml]`

use std::error::Error;

use nl_sim::conventions::PHYSICS_HZ;
use nl_sim::course::{BoundsSpec, CourseSpec, PieceSpec, Side, StartSpec};
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

const STANDARD_GRAVITY_M_S2: f32 = 9.81;
/// The curve rig: where it starts, how long it is and its lip angle.
const CURVE_START_M: f32 = 40.0;
const CURVE_LENGTH_M: f32 = 24.0;
const CURVE_ANGLE_DEG: f32 = 40.0;
/// Entry speeds for the curve rig, in m/s.
const CURVE_SPEEDS_M_S: [f32; 13] = [
    10.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 30.0, 32.0, 35.0, 40.0,
];
/// Drop heights above the resting ride height, in m.
const DROP_HEIGHTS_M: [f32; 7] = [0.25, 0.5, 0.8, 1.0, 1.5, 2.0, 3.0];
/// Wheel compression share counted as bottomed out (vehicle.toml's hard landing).
const BOTTOMED_SHARE: f32 = 0.95;

fn main() -> Result<(), Box<dyn Error>> {
    let config = load_vehicle_config()?;
    print_static_capacity(&config);
    print_curve_rig(&config)?;
    print_drops(&config)?;
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

/// The spring-only load at full travel and at the point where the body's flat underside
/// reaches the ground (mount height minus wheel radius below the body's lower face).
fn print_static_capacity(config: &VehicleConfig) {
    let wheels = &config.wheels;
    let mount_y_m = wheels.mount_points_m[0][1];
    let body_bottom_m = mount_y_m + config.chassis.half_extents_m[1];
    // The ray hits the ground at suspension length + radius below the mount.
    let length_at_body_contact_m = body_bottom_m - wheels.radius_m;
    let compression_at_body_m = wheels.suspension_rest_length_m - length_at_body_contact_m;
    let per_kg = wheels.suspension_stiffness_per_kg;
    let at_full_travel_g = 4.0 * per_kg * wheels.suspension_max_travel_m / STANDARD_GRAVITY_M_S2;
    let at_body_g = 4.0 * per_kg * compression_at_body_m / STANDARD_GRAVITY_M_S2;
    println!("Spring-only capacity (4 wheels, total load in g)");
    println!(
        "  body touches flat ground at {:.3} m compression ({:.0}% of travel): {:.2} g",
        compression_at_body_m,
        100.0 * compression_at_body_m / wheels.suspension_max_travel_m,
        at_body_g
    );
    println!(
        "  full travel {:.2} m: {:.2} g",
        wheels.suspension_max_travel_m, at_full_travel_g
    );
    println!();
}

/// A flat floor (small, so its contacts stay accurate) with `pieces`.
fn rig_course(pieces: Vec<PieceSpec>) -> CourseSpec {
    CourseSpec {
        name: "Suspension rig".to_string(),
        start: StartSpec {
            pos: [0.0, 0.7, 0.0],
            yaw: 0.0,
        },
        bounds: BoundsSpec {
            min: [-20.0, -20.0, -20.0],
            max: [200.0, 100.0, 20.0],
        },
        pieces,
    }
}

/// A settled car on `course`, its resting centre-of-mass height.
fn settled(config: &VehicleConfig, course: &CourseSpec) -> Result<(World, f32), Box<dyn Error>> {
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)?;
    let mut world =
        World::new_on_course(config, modes.realistic, course, DEFAULT_GROUND_FRICTION, 3);
    for _ in 0..PHYSICS_HZ {
        world.step(InputFrame::default());
    }
    let rest_y_m = world.chassis_pose().position_m[1];
    Ok((world, rest_y_m))
}

/// What one pass or drop did to the suspension and body.
#[derive(Default)]
struct LoadResult {
    peak_compression: f32,
    bottomed_steps: u32,
    /// Peak total spring load (stiffness × compression, all wheels), in g.
    peak_spring_g: f32,
    /// Peak total suspension force (spring and damper, each wheel capped), in g.
    peak_force_g: f32,
    body_touched: bool,
    max_body_speed_m_s: f32,
    crashed: bool,
}

impl LoadResult {
    /// Folds in this step's wheels and body contacts.
    fn record(&mut self, world: &World, config: &VehicleConfig, body_speed_m_s: f32) {
        let wheels = &config.wheels;
        let mut spring_m_s2 = 0.0;
        let mut force_n = 0.0;
        let mut bottomed = false;
        for wheel in world.wheel_diagnostics() {
            if !wheel.in_contact {
                continue;
            }
            let share = wheel.compression_m / wheels.suspension_max_travel_m;
            self.peak_compression = self.peak_compression.max(share);
            bottomed |= share >= BOTTOMED_SHARE;
            spring_m_s2 += wheels.suspension_stiffness_per_kg * wheel.compression_m.max(0.0);
            force_n += wheel
                .suspension_force_n
                .clamp(0.0, wheels.max_suspension_force_n);
        }
        self.bottomed_steps += u32::from(bottomed);
        self.peak_spring_g = self.peak_spring_g.max(spring_m_s2 / STANDARD_GRAVITY_M_S2);
        let weight_n = config.chassis.dry_mass_kg * STANDARD_GRAVITY_M_S2;
        self.peak_force_g = self.peak_force_g.max(force_n / weight_n);
        if world.chassis_touching() {
            self.body_touched = true;
            self.max_body_speed_m_s = self.max_body_speed_m_s.max(body_speed_m_s);
        }
        self.crashed |= world.is_crashed();
    }
}

fn print_curve_rig(config: &VehicleConfig) -> Result<(), Box<dyn Error>> {
    let radius_m = CURVE_LENGTH_M / libm::tanf(CURVE_ANGLE_DEG.to_radians());
    println!(
        "Curve rig: concave parabola, R = {radius_m:.1} m at the foot, {CURVE_ANGLE_DEG}° lip, no thrusters"
    );
    println!(
        "  speed  design_n_g  peak_comp  bottomed_steps  peak_spring_g  peak_force_g  body_touch  body_v  crashed"
    );
    for speed_m_s in CURVE_SPEEDS_M_S {
        let pass = curve_pass(config, speed_m_s)?;
        let design_g = 1.0 + speed_m_s * speed_m_s / radius_m / STANDARD_GRAVITY_M_S2;
        println!(
            "  {:5.1} {:11.2} {:10.2} {:15} {:14.2} {:13.2} {:11} {:7.2} {:8}",
            speed_m_s,
            design_g,
            pass.peak_compression,
            pass.bottomed_steps,
            pass.peak_spring_g,
            pass.peak_force_g,
            pass.body_touched,
            pass.max_body_speed_m_s,
            pass.crashed
        );
    }
    println!();
    Ok(())
}

/// Drives straight onto the curve rig at `speed_m_s` and measures until the lip.
fn curve_pass(config: &VehicleConfig, speed_m_s: f32) -> Result<LoadResult, Box<dyn Error>> {
    let rig = PieceSpec::Corkscrew {
        pos: [CURVE_START_M, 0.0, 0.0],
        yaw: 0.0,
        length: CURVE_LENGTH_M,
        width: 8.0,
        angle_deg: CURVE_ANGLE_DEG,
        bank_deg: 1.0,
        side: Side::Right,
    };
    let (mut world, rest_y_m) = settled(config, &rig_course(vec![rig]))?;
    let start_x_m = CURVE_START_M - 10.0 - 0.3 * speed_m_s;
    world.place_car(
        [start_x_m, rest_y_m, 0.0],
        [0.0, 0.0, 0.0, 1.0],
        [speed_m_s, 0.0, 0.0],
        [0.0; 3],
    )?;
    let mut result = LoadResult::default();
    let lip_x_m = CURVE_START_M + CURVE_LENGTH_M;
    for _ in 0..(10 * PHYSICS_HZ) {
        let error_m_s = speed_m_s - world.forward_speed_m_s();
        let throttle = ((error_m_s * 0.5).clamp(-1.0, 1.0) * 127.0).round() as i8;
        let velocity_before = world.linear_velocity_m_s();
        world.step(InputFrame {
            throttle,
            ..InputFrame::default()
        });
        let body_speed_m_s = speed_into_ground_m_s(&world, velocity_before);
        result.record(&world, config, body_speed_m_s);
        if world.is_crashed() || world.chassis_pose().position_m[0] > lip_x_m {
            break;
        }
    }
    Ok(result)
}

/// The largest speed into the ground along any body contact normal, from the velocity
/// before the step (centre-of-mass velocity only: these rigs barely rotate).
fn speed_into_ground_m_s(world: &World, velocity_m_s: [f32; 3]) -> f32 {
    let mut fastest_m_s: f32 = 0.0;
    for point in world.chassis_contact_points() {
        let along = velocity_m_s[0] * point.normal[0]
            + velocity_m_s[1] * point.normal[1]
            + velocity_m_s[2] * point.normal[2];
        fastest_m_s = fastest_m_s.max(along);
    }
    fastest_m_s
}

fn print_drops(config: &VehicleConfig) -> Result<(), Box<dyn Error>> {
    println!("Flat drops from rest (level, no thrusters): rebound must stay below the drop");
    println!(
        "  height_m  fall_m_s  peak_comp  bottomed_steps  peak_force_g  body_touch  body_v  rebound_m  crashed"
    );
    for height_m in DROP_HEIGHTS_M {
        let (drop, rebound_m) = flat_drop(config, height_m)?;
        let fall_m_s = (2.0 * STANDARD_GRAVITY_M_S2 * height_m).sqrt();
        println!(
            "  {:8.2} {:9.2} {:10.2} {:15} {:13.2} {:11} {:7.2} {:10.3} {:8}",
            height_m,
            fall_m_s,
            drop.peak_compression,
            drop.bottomed_steps,
            drop.peak_force_g,
            drop.body_touched,
            drop.max_body_speed_m_s,
            rebound_m,
            drop.crashed
        );
    }
    Ok(())
}

/// Drops the car level from `height_m` above its ride height; returns the load record
/// and how high the centre of mass bounced above the ride height afterwards, in m.
fn flat_drop(config: &VehicleConfig, height_m: f32) -> Result<(LoadResult, f32), Box<dyn Error>> {
    let (mut world, rest_y_m) = settled(config, &rig_course(Vec::new()))?;
    world.place_car(
        [0.0, rest_y_m + height_m, 0.0],
        [0.0, 0.0, 0.0, 1.0],
        [0.0; 3],
        [0.0; 3],
    )?;
    let mut result = LoadResult::default();
    let mut touched = false;
    let mut lowest_m = f32::INFINITY;
    let mut rebound_m: f32 = 0.0;
    for _ in 0..(4 * PHYSICS_HZ) {
        let velocity_before = world.linear_velocity_m_s();
        world.step(InputFrame::default());
        let body_speed_m_s = speed_into_ground_m_s(&world, velocity_before);
        result.record(&world, config, body_speed_m_s);
        let y_m = world.chassis_pose().position_m[1];
        touched |= world.wheels_touching_count() > 0;
        lowest_m = lowest_m.min(y_m);
        // The rebound: the highest point after the car started coming back up.
        if touched && y_m > lowest_m + 0.01 {
            rebound_m = rebound_m.max(y_m - rest_y_m);
        }
    }
    Ok((result, rebound_m))
}
