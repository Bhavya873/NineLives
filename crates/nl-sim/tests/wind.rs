//! Wind (SPEC §7.2): sign tests written before the code (CLAUDE.md golden rule 5). A
//! steady side wind pushes a car in the air downwind and, because the centre of
//! pressure sits behind and above the centre of mass, turns its nose into the wind and
//! leans its top downwind. Gusts are seeded, so a seed always gives the same gusts, and
//! the Off level gives no wind at all.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, realistic, scenarios, vehicle};
use nl_sim::conventions::{PHYSICS_DT_S, PHYSICS_HZ};
use nl_sim::input::InputFrame;
use nl_sim::wind::{Wind, WindLevel, WindLevelConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// Wind heading that blows towards world +Z: yaw is measured like the car's, 0 along +X
/// and positive turning left (towards −Z), so −90° points along +Z.
const TOWARDS_PLUS_Z_RAD: f32 = -std::f32::consts::FRAC_PI_2;

/// The repo's car 10 m up, level, still, facing world +X.
fn airborne(seed: u64) -> World {
    let mut world =
        World::new_on_flat_ground(&vehicle(), realistic(), DEFAULT_GROUND_FRICTION, seed);
    world
        .place_car([0.0, 10.0, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
    world
}

fn run(world: &mut World, seconds: f32) {
    for _ in 0..(seconds * PHYSICS_HZ as f32).round() as usize {
        world.step(InputFrame::default());
    }
}

/// A steady wind: no gusts.
fn steady(speed_m_s: f32) -> WindLevelConfig {
    WindLevelConfig {
        mean_speed_m_s: speed_m_s,
        gust_std_m_s: 0.0,
        gust_time_constant_s: 1.0,
    }
}

#[test]
fn a_steady_side_wind_blows_a_car_in_the_air_downwind() {
    let mut world = airborne(1_000_001);
    world.set_wind(steady(15.0), TOWARDS_PLUS_Z_RAD);

    run(&mut world, 0.5);

    assert!(world.wind_m_s()[2] > 14.9, "the wind blows along +Z");
    let [x, _, z] = world.linear_velocity_m_s();
    assert!(z > 0.1, "drifts downwind (+Z), got {z}");
    assert!(x.abs() < 0.05, "no push along the nose, got {x}");
}

#[test]
fn a_side_wind_turns_the_nose_into_the_wind_and_leans_the_top_downwind() {
    let mut world = airborne(1_000_002);
    world.set_wind(steady(15.0), TOWARDS_PLUS_Z_RAD);

    run(&mut world, 0.3);

    // The centre of pressure is behind the centre of mass, so the tail is blown to +Z
    // and the nose swings to −Z, upwind: positive yaw about +Y (SPEC §4).
    let [roll_rad_s, yaw_rad_s, _] = world.angular_velocity_body_rad_s();
    assert!(yaw_rad_s > 0.01, "yaw rate {yaw_rad_s}");
    // It is also above the centre of mass, so the top is pushed to +Z (the right side):
    // positive roll about +X turns +Y towards +Z.
    assert!(roll_rad_s > 0.001, "roll rate {roll_rad_s}");
}

#[test]
fn the_same_seed_gives_the_same_gusts_and_another_seed_different_ones() {
    let storm = scenarios().wind.level(WindLevel::Storm);
    let gusts = |seed: u64| {
        let mut wind = Wind::calm(seed);
        wind.set_level(storm);
        let mut samples = Vec::new();
        for _ in 0..600 {
            wind.step(PHYSICS_DT_S);
            samples.push(wind.velocity_m_s());
        }
        samples
    };

    let first = gusts(1_000_003);

    assert_eq!(first, gusts(1_000_003));
    assert_ne!(first, gusts(1_000_004));
    let changes = first.windows(2).filter(|pair| pair[0] != pair[1]).count();
    assert!(changes > 500, "a storm gusts: the wind keeps changing");
}

#[test]
fn gusts_wander_around_the_mean_wind_with_about_the_configured_size() {
    let storm = scenarios().wind.level(WindLevel::Storm);
    let mut wind = Wind::calm(1_000_005);
    wind.set_level(storm);
    let mean = wind.mean_velocity_m_s();

    let mut squared_sum = 0.0_f64;
    let steps = 120 * 600;
    for _ in 0..steps {
        wind.step(PHYSICS_DT_S);
        let [x, _, z] = wind.velocity_m_s();
        let (dx, dz) = (x - mean[0], z - mean[2]);
        squared_sum += f64::from(dx * dx + dz * dz) / 2.0;
    }

    let std_m_s = (squared_sum / steps as f64).sqrt() as f32;
    let expected = storm.gust_std_m_s;
    assert!(
        (std_m_s - expected).abs() < 0.3 * expected,
        "gust size {std_m_s} vs configured {expected}"
    );
}

#[test]
fn the_off_level_gives_no_wind_and_no_push() {
    let mut world = airborne(1_000_006);
    let off = scenarios().wind.level(WindLevel::Off);
    world.set_wind_level(off);

    run(&mut world, 0.5);

    assert_eq!(world.wind_m_s(), [0.0; 3]);
    let [x, _, z] = world.linear_velocity_m_s();
    assert!(x.abs() < 1e-4 && z.abs() < 1e-4);
}

#[test]
fn wind_levels_get_stronger_from_off_to_storm() {
    let wind = scenarios().wind;

    let speeds = [WindLevel::Off, WindLevel::Breeze, WindLevel::Storm]
        .map(|level| wind.level(level).mean_speed_m_s);

    assert_eq!(speeds[0], 0.0);
    assert!(speeds[0] < speeds[1] && speeds[1] < speeds[2]);
    assert_eq!(WindLevel::from_name("storm"), Some(WindLevel::Storm));
    assert_eq!(WindLevel::from_name("hurricane"), None);
}

#[test]
fn a_restored_snapshot_keeps_the_gusts_going_the_same_way() {
    let storm = scenarios().wind.level(WindLevel::Storm);
    let mut world = airborne(1_000_007);
    world.set_wind_level(storm);
    run(&mut world, 0.2);
    let snapshot = world.snapshot().unwrap();
    run(&mut world, 0.5);
    let expected = world.wind_m_s();

    world.restore(&snapshot).unwrap();
    run(&mut world, 0.5);

    assert_eq!(world.wind_m_s(), expected);
}
