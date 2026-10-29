//! The survival rule (docs/contracts/survival.md), reduced: a clean entry at 2 to 55.6 m/s
//! onto each covered piece, PD flying, both modes, a few seeds, never crashes. The full
//! sweep is `cargo run --release -p nl-control --example speed_sweep` (20 seeds, 24 speeds);
//! its result is `results/speed_sweep_after_pieces.json`. Flip plates are not covered.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_control::config::DEFAULT_CONTROLLERS_TOML;
use nl_control::{ControlCtx, Controller, build_controller};
use nl_sim::conventions::PHYSICS_HZ;
use nl_sim::course::{BoundsSpec, CourseSpec, PieceSpec, Side, StartSpec};
use nl_sim::course_build::{MIN_CONCAVE_RADIUS_M, ramp_lip_height_m};
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

const PIECE_X_M: f32 = 60.0;
const SPEEDS_M_S: [f32; 7] = [2.0, 5.0, 9.9, 20.0, 40.0, 55.5, 55.6];
const SEEDS: u64 = 3;
const RUN_S: u32 = 12;
/// A table top is covered up to the height whose landing stays under about 9 m/s at
/// 200 km/h: v_s^2 + 2 g H <= 81 gives H <= 1.07 m (ramp-curvature.md).
const WALL_HEIGHT_M: f32 = 1.0;

fn ramp(length: f32, angle_deg: f32) -> PieceSpec {
    PieceSpec::Ramp {
        pos: [PIECE_X_M, 0.0, 0.0],
        yaw: 0.0,
        length,
        width: 8.0,
        angle_deg,
        v_complete_min_m_s: 0.0,
    }
}

fn arc_length(angle_deg: f32) -> f32 {
    MIN_CONCAVE_RADIUS_M * angle_deg.to_radians()
}

/// Each covered piece alone on a long floor, as the sweep lays them.
fn cases() -> Vec<(&'static str, Vec<PieceSpec>)> {
    let ramp_m = arc_length(6.0);
    vec![
        ("ramp_8", vec![ramp(arc_length(8.0), 8.0)]),
        (
            "ramp_gap",
            vec![
                ramp(ramp_m, 6.0),
                PieceSpec::Gap {
                    pos: [PIECE_X_M + ramp_m, 0.0, 0.0],
                    length: 6.0,
                    pit_depth: 0.5,
                    entry_height: ramp_lip_height_m(ramp_m, 6.0),
                },
            ],
        ),
        (
            "kicker",
            vec![PieceSpec::Kicker {
                pos: [PIECE_X_M, 0.0, 0.0],
                yaw: 0.0,
                length: arc_length(8.0),
                width: 4.0,
                angle_deg: 8.0,
                side: Side::Left,
            }],
        ),
        (
            "bumps_0.4",
            vec![PieceSpec::Bumps {
                pos: [PIECE_X_M, 0.0, 0.0],
                yaw: 0.0,
                count: 4,
                height: 0.4,
            }],
        ),
        (
            "corkscrew",
            vec![PieceSpec::Corkscrew {
                pos: [PIECE_X_M, 0.0, 0.0],
                yaw: 0.0,
                length: MIN_CONCAVE_RADIUS_M * 8.0_f32.to_radians().tan(),
                width: 4.0,
                angle_deg: 8.0,
                bank_deg: 8.0,
                side: Side::Right,
            }],
        ),
        (
            "halfpipe",
            vec![PieceSpec::Halfpipe {
                pos: [PIECE_X_M, 0.0, 0.0],
                yaw: 0.0,
                radius: 6.0,
                length: 20.0,
            }],
        ),
        (
            "wall_ramp",
            vec![PieceSpec::Wall {
                pos: [PIECE_X_M, 0.0, 0.0],
                yaw: 0.0,
                height: WALL_HEIGHT_M,
                with_ramp: true,
            }],
        ),
    ]
}

/// Runs one clean entry and says whether the car crashed.
fn crashes(pieces: &[PieceSpec], arcade: bool, speed_m_s: f32, seed: u64) -> bool {
    let course = CourseSpec {
        name: "survival".to_string(),
        start: StartSpec {
            pos: [0.0, 0.7, 0.0],
            yaw: 0.0,
        },
        bounds: BoundsSpec {
            min: [-800.0, -20.0, -800.0],
            max: [800.0, 300.0, 800.0],
        },
        pieces: pieces.to_vec(),
    };
    let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML).unwrap();
    let mode = if arcade {
        modes.arcade
    } else {
        modes.realistic
    };
    let mut world = World::new_on_course(&vehicle, mode, &course, DEFAULT_GROUND_FRICTION, seed);
    for _ in 0..PHYSICS_HZ {
        world.step(InputFrame::default());
    }
    let rest_y_m = world.chassis_pose().position_m[1];
    // Seed 0 enters dead centre; others a little off, inside the clean limits.
    let lateral_m = [0.0, 0.4, -0.4][seed as usize % 3];
    let lead_m = 10.0 + 0.5 * speed_m_s;
    world
        .place_car(
            [PIECE_X_M - lead_m, rest_y_m, lateral_m],
            [0.0, 0.0, 0.0, 1.0],
            [speed_m_s, 0.0, 0.0],
            [0.0; 3],
        )
        .unwrap();
    world.refill_tank();
    let mut pd = build_controller("pd", &vehicle, mode, DEFAULT_CONTROLLERS_TOML, None).unwrap();
    pd.reset(seed);
    for _ in 0..(RUN_S * PHYSICS_HZ) {
        let error = speed_m_s - world.forward_speed_m_s();
        let input = InputFrame {
            throttle: ((error * 0.5).clamp(-1.0, 1.0) * 127.0).round() as i8,
            ..InputFrame::default()
        };
        if world.is_control_step() {
            let ctx = ControlCtx::for_world(&world, input);
            world.set_controller_throttles(pd.act(&ctx));
        }
        world.step(input);
        if world.is_crashed() {
            return true;
        }
    }
    false
}

#[test]
fn clean_entries_up_to_200_kmh_never_crash_on_covered_pieces() {
    let mut crashed = Vec::new();
    for (name, pieces) in cases() {
        for arcade in [false, true] {
            for speed_m_s in SPEEDS_M_S {
                for seed in 0..SEEDS {
                    if crashes(&pieces, arcade, speed_m_s, seed) {
                        crashed.push(format!(
                            "{name} arcade={arcade} {speed_m_s} m/s seed {seed}"
                        ));
                    }
                }
            }
        }
    }
    assert!(crashed.is_empty(), "crashes: {crashed:#?}");
}
