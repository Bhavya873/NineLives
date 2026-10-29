//! The suspension bump stop (plan 2026-10-27 G2, `wheels.bump_stop_*` in vehicle.toml):
//! it must not add energy (a drop rebounds no higher or faster than it fell) and must
//! not make the car oscillate more and more over a long bump strip at 55 m/s.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, realistic, vehicle};
use nl_sim::conventions::PHYSICS_HZ;
use nl_sim::course::{BoundsSpec, CourseSpec, PieceSpec, StartSpec};
use nl_sim::input::InputFrame;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

const HZ: usize = PHYSICS_HZ as usize;

fn course(pieces: Vec<PieceSpec>) -> CourseSpec {
    CourseSpec {
        name: "bump stop rig".to_string(),
        start: StartSpec {
            pos: [0.0, 0.7, 0.0],
            yaw: 0.0,
        },
        bounds: BoundsSpec {
            min: [-50.0, -20.0, -300.0],
            max: [800.0, 100.0, 300.0],
        },
        pieces,
    }
}

fn settled_world(pieces: Vec<PieceSpec>) -> (World, f32) {
    let mut world = World::new_on_course(
        &vehicle(),
        realistic(),
        &course(pieces),
        DEFAULT_GROUND_FRICTION,
        1,
    );
    for _ in 0..HZ {
        world.step(InputFrame::default());
    }
    let rest_y_m = world.chassis_pose().position_m[1];
    (world, rest_y_m)
}

/// Drops the car level from `height_m` above its ride height. Returns the fall speed just
/// before the wheels first touch, the fastest upward speed after the bottom of the
/// compression, and the highest rebound of the centre of mass above ride height, all in
/// metric units (m/s, m/s, m).
fn drop_car(height_m: f32) -> (f32, f32, f32) {
    let (mut world, rest_y_m) = settled_world(Vec::new());
    world
        .place_car([0.0, rest_y_m + height_m, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
    let mut fall_m_s: f32 = 0.0;
    let mut touched = false;
    let mut lowest_m = f32::INFINITY;
    let mut up_m_s: f32 = 0.0;
    let mut rebound_m: f32 = 0.0;
    for _ in 0..(4 * HZ) {
        let vy_before = world.linear_velocity_m_s()[1];
        world.step(InputFrame::default());
        if !touched {
            fall_m_s = fall_m_s.max(-vy_before);
        }
        touched |= world.wheels_touching_count() > 0;
        let y_m = world.chassis_pose().position_m[1];
        lowest_m = lowest_m.min(y_m);
        if touched && y_m > lowest_m + 0.01 {
            up_m_s = up_m_s.max(world.linear_velocity_m_s()[1]);
            rebound_m = rebound_m.max(y_m - rest_y_m);
        }
    }
    (fall_m_s, up_m_s, rebound_m)
}

#[test]
fn a_drop_rebounds_with_less_energy_than_it_fell_with() {
    for height_m in [0.25, 0.5, 1.0, 1.5, 2.0] {
        let (fall_m_s, up_m_s, rebound_m) = drop_car(height_m);
        assert!(
            up_m_s <= fall_m_s,
            "drop {height_m} m: left the bottom at {up_m_s} m/s, arrived at {fall_m_s} m/s"
        );
        assert!(
            rebound_m <= height_m,
            "drop {height_m} m: rebounded {rebound_m} m, higher than it fell"
        );
    }
}

#[test]
fn the_bump_stop_adds_no_bounce_to_a_drop_the_stock_spring_can_take() {
    // 0.5 m is well inside the stock spring's range: the stop barely engages, so the
    // rebound stays tiny (R2: 0.004 m).
    let (_, _, rebound_m) = drop_car(0.5);
    assert!(rebound_m < 0.05, "rebound {rebound_m} m");
}

/// Height of the test bumps, in m. Low enough that a bare car stays on the road, high
/// enough that the wheels compress past the bump stop's start (0.14 m).
const BUMP_HEIGHT_M: f32 = 0.12;

/// What a 10 s pass over bump strips at 55 m/s did.
struct StripPass {
    /// Peak vertical speed of the body in each second, in m/s.
    peak_vertical_m_s: Vec<f32>,
    /// Largest wheel compression seen, in m.
    peak_compression_m: f32,
    crashed: bool,
}

/// Drives `strips` strips of four bumps, one every 30 m, at 55 m/s for 10 s.
fn drive_bump_strips(strips: u32) -> StripPass {
    let pieces: Vec<PieceSpec> = (0..strips)
        .map(|index| PieceSpec::Bumps {
            pos: [40.0 + 30.0 * index as f32, 0.0, 0.0],
            yaw: 0.0,
            count: 4,
            height: BUMP_HEIGHT_M,
        })
        .collect();
    let (mut world, rest_y_m) = settled_world(pieces);
    let speed_m_s = 55.0;
    world
        .place_car([0.0, rest_y_m, 0.0], LEVEL, [speed_m_s, 0.0, 0.0], [0.0; 3])
        .unwrap();
    let mut pass = StripPass {
        peak_vertical_m_s: vec![0.0; 10],
        peak_compression_m: 0.0,
        crashed: false,
    };
    for step in 0..(10 * HZ) {
        let error_m_s = speed_m_s - world.forward_speed_m_s();
        let throttle = ((error_m_s * 0.5).clamp(-1.0, 1.0) * 127.0).round() as i8;
        world.step(InputFrame {
            throttle,
            ..InputFrame::default()
        });
        let peak = &mut pass.peak_vertical_m_s[step / HZ];
        *peak = peak.max(world.linear_velocity_m_s()[1].abs());
        for wheel in world.wheel_diagnostics() {
            if wheel.in_contact {
                pass.peak_compression_m = pass.peak_compression_m.max(wheel.compression_m);
            }
        }
        if world.is_crashed() {
            pass.crashed = true;
            break;
        }
    }
    pass
}

#[test]
fn ten_seconds_over_bump_strips_at_55_m_s_do_not_grow_an_oscillation() {
    let pass = drive_bump_strips(19);
    let peaks = &pass.peak_vertical_m_s;
    assert!(!pass.crashed, "crashed on bumps at 55 m/s: {peaks:?}");
    assert!(
        pass.peak_compression_m > vehicle().wheels.bump_stop_start_m,
        "the bumps never reached the bump stop (peak {} m)",
        pass.peak_compression_m
    );
    let early = peaks[..3].iter().cloned().fold(0.0_f32, f32::max);
    let late = peaks[7..].iter().cloned().fold(0.0_f32, f32::max);
    assert!(
        late <= early * 1.5 + 0.5,
        "vertical motion grows: first 3 s peak {early} m/s, last 3 s peak {late} m/s ({peaks:?})"
    );
}
