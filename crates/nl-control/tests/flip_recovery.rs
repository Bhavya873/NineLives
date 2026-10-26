//! Flip recovery from start to landing (SPEC §8, §12): the car is let go mid-air in a
//! tumble, and the controller must land it on its wheels in both modes. Off crashes every
//! tumble; PD lands all but a car hanging exactly upside down and still, which Hybrid's AI
//! covers. The Hybrid tests run the shipped models/policy.json, so a new policy must pass.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use std::sync::{Arc, OnceLock};

use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};
use nl_control::{Ai, ControlCtx, Controller, Hybrid, HybridGateSettings, Off, Pd};
use nl_policy::Policy;
use nl_sim::conventions::PHYSICS_HZ;
use nl_sim::events::SimEvent;
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModeParams, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// Longest a recovery may take before the test gives up, in seconds.
const RECOVERY_LIMIT_S: f32 = 8.0;
/// Most AI↔PD handovers one recovery may need: to the AI, then back to PD.
const MOST_HANDOVERS: usize = 2;

const FORWARD: [f32; 3] = [1.0, 0.0, 0.0];
const RIGHT: [f32; 3] = [0.0, 0.0, 1.0];
const LEVEL: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// Where and how the car is let go: height, attitude and spin.
struct Tumble {
    name: &'static str,
    height_m: f32,
    /// Unit quaternion [x, y, z, w].
    rotation: [f32; 4],
    /// World-frame spin in rad/s (X roll, Y yaw, Z pitch at the start).
    spin_rad_s: [f32; 3],
}

/// How one recovery ended.
#[derive(Debug, PartialEq)]
enum Outcome {
    Landed,
    Crashed,
    /// Neither a save nor a crash within `RECOVERY_LIMIT_S`.
    TimedOut,
}

/// A rotation of `angle_deg` about unit `axis`, as [x, y, z, w].
fn turned(axis: [f32; 3], angle_deg: f32) -> [f32; 4] {
    let half_rad = angle_deg.to_radians() / 2.0;
    let sin = half_rad.sin();
    [axis[0] * sin, axis[1] * sin, axis[2] * sin, half_rad.cos()]
}

/// Tumbles PD lands: every way of flipping except hanging exactly upside down and still.
/// (A car let go on its side is already in pd.rs.)
fn tumbles_pd_lands() -> Vec<Tumble> {
    vec![
        Tumble {
            name: "nose-down",
            height_m: 6.0,
            rotation: turned(RIGHT, -90.0),
            spin_rad_s: [0.0; 3],
        },
        Tumble {
            name: "tail-down",
            height_m: 6.0,
            rotation: turned(RIGHT, 90.0),
            spin_rad_s: [0.0; 3],
        },
        Tumble {
            name: "barrel roll",
            height_m: 6.0,
            rotation: LEVEL,
            spin_rad_s: [4.0, 0.0, 0.0],
        },
        Tumble {
            name: "front flip",
            height_m: 6.0,
            rotation: LEVEL,
            spin_rad_s: [0.0, 0.0, -4.0],
        },
        Tumble {
            name: "back flip",
            height_m: 6.0,
            rotation: LEVEL,
            spin_rad_s: [0.0, 0.0, 4.0],
        },
        Tumble {
            name: "flat spin while tipped 70°",
            height_m: 6.0,
            rotation: turned(FORWARD, 70.0),
            spin_rad_s: [0.0, 4.0, 0.0],
        },
        Tumble {
            name: "upside down and back-flipping",
            height_m: 8.0,
            rotation: turned(FORWARD, 180.0),
            spin_rad_s: [0.0, 0.0, 3.0],
        },
    ]
}

/// Upside down and still: PD's blind spot, which Hybrid's AI must cover.
fn upside_down_and_still(height_m: f32) -> Tumble {
    Tumble {
        name: "upside down and still",
        height_m,
        rotation: turned(FORWARD, 180.0),
        spin_rad_s: [0.0; 3],
    }
}

fn vehicle() -> VehicleConfig {
    VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap()
}

fn both_modes() -> [(&'static str, ModeParams); 2] {
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML).unwrap();
    [("Realistic", modes.realistic), ("Arcade", modes.arcade)]
}

fn pd_for(mode: ModeParams) -> Pd {
    let controllers = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();
    Pd::for_vehicle(&vehicle(), mode, &controllers)
}

/// The shipped policy (models/policy.json), parsed once for every test in this file.
fn shipped_policy() -> Arc<Policy> {
    static POLICY: OnceLock<Arc<Policy>> = OnceLock::new();
    let policy = POLICY.get_or_init(|| {
        Arc::new(Policy::from_json(include_str!("../../../models/policy.json")).unwrap())
    });
    Arc::clone(policy)
}

/// Hybrid with the shipped policy.
fn hybrid_for(mode: ModeParams) -> Hybrid {
    Hybrid::new(
        pd_for(mode),
        Ai::new(shipped_policy()),
        HybridGateSettings::for_vehicle(&vehicle()),
    )
}

/// A world with the car let go in `tumble`.
fn let_go(mode: ModeParams, tumble: &Tumble) -> World {
    let mut world = World::new_on_flat_ground(&vehicle(), mode, DEFAULT_GROUND_FRICTION, 0);
    world
        .place_car(
            [0.0, tumble.height_m, 0.0],
            tumble.rotation,
            [0.0; 3],
            tumble.spin_rad_s,
        )
        .unwrap();
    world
}

/// One physics step with `controller` acting at 60 Hz; returns how the recovery ended,
/// if it ended on this step.
fn step_once(world: &mut World, controller: &mut dyn Controller) -> Option<Outcome> {
    if world.is_control_step() {
        let ctx = ControlCtx::for_world(world, InputFrame::default());
        world.set_controller_throttles(controller.act(&ctx));
    }
    world.step(InputFrame::default());
    for event in world.drain_events() {
        match event {
            SimEvent::Crash { .. } => return Some(Outcome::Crashed),
            SimEvent::Save { .. } => return Some(Outcome::Landed),
            _ => {}
        }
    }
    None
}

/// Physics steps in `RECOVERY_LIMIT_S`.
fn recovery_steps() -> usize {
    (RECOVERY_LIMIT_S * PHYSICS_HZ as f32) as usize
}

/// Lets the car go in `tumble` and runs `controller` until a save, a crash or
/// `RECOVERY_LIMIT_S`.
fn recover(mode: ModeParams, tumble: &Tumble, controller: &mut dyn Controller) -> Outcome {
    let mut world = let_go(mode, tumble);
    controller.reset(0);
    for _ in 0..recovery_steps() {
        if let Some(outcome) = step_once(&mut world, controller) {
            return outcome;
        }
    }
    Outcome::TimedOut
}

/// Like `recover` with Hybrid, also counting how often its gate switches between PD
/// and the AI.
fn recover_with_hybrid(mode: ModeParams, tumble: &Tumble) -> (Outcome, usize) {
    let mut world = let_go(mode, tumble);
    let mut hybrid = hybrid_for(mode);
    hybrid.reset(0);
    let mut handovers = 0;
    let mut ai_was_in_control = false;
    for _ in 0..recovery_steps() {
        let ended = step_once(&mut world, &mut hybrid);
        if hybrid.ai_in_control() != ai_was_in_control {
            handovers += 1;
            ai_was_in_control = hybrid.ai_in_control();
        }
        if let Some(outcome) = ended {
            return (outcome, handovers);
        }
    }
    (Outcome::TimedOut, handovers)
}

#[test]
fn with_the_thrusters_off_every_tumble_crashes() {
    for (mode_name, mode) in both_modes() {
        for tumble in tumbles_pd_lands() {
            let outcome = recover(mode, &tumble, &mut Off);

            assert_eq!(outcome, Outcome::Crashed, "{mode_name} {}", tumble.name);
        }
    }
}

#[test]
fn pd_lands_every_tumble_except_hanging_still_upside_down_in_both_modes() {
    for (mode_name, mode) in both_modes() {
        for tumble in tumbles_pd_lands() {
            let mut pd = pd_for(mode);

            let outcome = recover(mode, &tumble, &mut pd);

            assert_eq!(outcome, Outcome::Landed, "{mode_name} {}", tumble.name);
        }
    }
}

#[test]
fn hybrid_lands_every_tumble_pd_lands_and_hands_over_at_most_twice() {
    for (mode_name, mode) in both_modes() {
        for tumble in tumbles_pd_lands() {
            let (outcome, handovers) = recover_with_hybrid(mode, &tumble);

            assert_eq!(outcome, Outcome::Landed, "{mode_name} {}", tumble.name);
            assert!(
                handovers <= MOST_HANDOVERS,
                "{mode_name} {}: {handovers} handovers",
                tumble.name
            );
        }
    }
}

#[test]
fn hybrid_lands_a_car_let_go_upside_down_and_still() {
    for (mode_name, mode) in both_modes() {
        for height_m in [6.0, 8.0] {
            let tumble = upside_down_and_still(height_m);

            let (hybrid_outcome, _) = recover_with_hybrid(mode, &tumble);

            // PD alone crashes here today (its attitude error has no preferred way round,
            // so it only nudges the roll). That is not pinned: a better PD may land it.
            assert_eq!(
                hybrid_outcome,
                Outcome::Landed,
                "{mode_name} Hybrid {height_m} m"
            );
        }
    }
}
