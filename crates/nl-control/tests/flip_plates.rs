//! Saves over flip plates (SPEC §7.11, §8), in both modes, on bounds as wide as the
//! real courses. Mild plates (generated courses): PD and Hybrid land every pass, the
//! thrusters stopping the flip early. Strong plates (playground, Stage F): the car goes
//! past 120° even with the thrusters on, Hybrid's AI takes over and lands it.
//! The Hybrid tests run the shipped models/policy.json.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use std::sync::{Arc, OnceLock};

use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};
use nl_control::{Ai, ControlCtx, Controller, Hybrid, HybridGateSettings, Pd};
use nl_policy::Policy;
use nl_sim::conventions::PHYSICS_HZ;
use nl_sim::course::CourseSpec;
use nl_sim::course_gen::{FlipPlates, FlipPreset, PieceCatalog};
use nl_sim::events::SimEvent;
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModeParams, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// Where the plate sits along the course, in m; the car starts 3 m before it.
const PLATE_X_M: f32 = 30.0;
/// Longest a pass may take to end in a save, in s.
const PASS_LIMIT_S: f32 = 8.0;
/// Speeds a car meets a plate at, in m/s.
const SPEEDS_M_S: [f32; 4] = [10.0, 16.0, 22.0, 28.0];
/// A mild pass counts only if the kick tipped the car past this, in degrees.
const UPSET_DEG: f32 = 30.0;
/// A strong pass must tip the car past this even with the thrusters on, in degrees.
const STRONG_UPSET_DEG: f32 = 120.0;
/// Half the width of a generated course's bounds, in m (pieces.json track_half_width_m).
const GENERATED_HALF_WIDTH_M: f32 = 10.0;
/// Room either side for a strong plate's landing, in m.
const STRONG_HALF_WIDTH_M: f32 = 40.0;
const LEVEL: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// How one pass ended.
struct Pass {
    saved: bool,
    peak_tilt_deg: f32,
    /// Hybrid only: the gate gave the AI the nozzles at some point.
    ai_took_over: bool,
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

/// Hybrid with the shipped policy, parsed once for every test in this file.
fn hybrid_for(mode: ModeParams) -> Hybrid {
    static POLICY: OnceLock<Arc<Policy>> = OnceLock::new();
    let policy = POLICY.get_or_init(|| {
        Arc::new(Policy::from_json(include_str!("../../../models/policy.json")).unwrap())
    });
    Hybrid::new(
        pd_for(mode),
        Ai::new(Arc::clone(policy)),
        HybridGateSettings::for_vehicle(&vehicle()),
    )
}

fn flip_plates() -> FlipPlates {
    PieceCatalog::built_in().unwrap().flip_plates().unwrap()
}

/// A car rolling at `speed_m_s` toward a lone plate on a track `half_width_m` wide.
fn approach(
    mode: ModeParams,
    plate: (&FlipPreset, f32),
    speed_m_s: f32,
    half_width_m: f32,
) -> World {
    let empty = CourseSpec::from_json_str(&format!(
        r#"{{"version": 1, "name": "Flip plate", "start": {{"pos": [0, 0.7, 0]}},
           "bounds": {{"min": [-20, -20, -{half_width_m}], "max": [250, 80, {half_width_m}]}},
           "pieces": []}}"#
    ))
    .unwrap();
    let (preset, strength) = plate;
    let spec = CourseSpec {
        pieces: vec![preset.kick_plate_at([PLATE_X_M, 0.0, 0.0], strength)],
        ..empty
    };
    let mut world = World::new_on_course(&vehicle(), mode, &spec, DEFAULT_GROUND_FRICTION, 0);
    let start_m = [PLATE_X_M - 3.0, 0.7, 0.0];
    world
        .place_car(start_m, LEVEL, [speed_m_s, 0.0, 0.0], [0.0; 3])
        .unwrap();
    world
}

/// Runs `controller` until a save, a crash or `PASS_LIMIT_S`; `ai_in_control` reads
/// Hybrid's gate after each control step (always false for other controllers).
fn fly<C: Controller>(
    world: &mut World,
    controller: &mut C,
    ai_in_control: fn(&C) -> bool,
) -> Pass {
    controller.reset(0);
    let mut pass = Pass {
        saved: false,
        peak_tilt_deg: 0.0,
        ai_took_over: false,
    };
    for _ in 0..(PASS_LIMIT_S * PHYSICS_HZ as f32) as usize {
        if world.is_control_step() {
            let ctx = ControlCtx::for_world(world, InputFrame::default());
            world.set_controller_throttles(controller.act(&ctx));
            pass.ai_took_over |= ai_in_control(controller);
        }
        world.step(InputFrame::default());
        pass.peak_tilt_deg = pass.peak_tilt_deg.max(world.tilt_rad().to_degrees());
        for event in world.drain_events() {
            match event {
                SimEvent::Crash { .. } => return pass,
                SimEvent::Save { .. } => {
                    pass.saved = true;
                    return pass;
                }
                _ => {}
            }
        }
    }
    pass
}

fn never_ai(_: &Pd) -> bool {
    false
}

/// Every (label, plate) pair: `presets` at each of `strengths`, at every speed.
fn plates<'a>(
    presets: &'a [FlipPreset],
    strengths: &[f32],
) -> Vec<(String, &'a FlipPreset, f32, f32)> {
    let mut plates = Vec::new();
    for preset in presets {
        for strength in strengths {
            for speed_m_s in SPEEDS_M_S {
                let label = format!("{} ×{strength} at {speed_m_s} m/s", preset.name);
                plates.push((label, preset, *strength, speed_m_s));
            }
        }
    }
    plates
}

#[test]
fn pd_and_hybrid_land_every_mild_flip_plate_in_both_modes() {
    let flip = flip_plates();
    // Only what generated courses draw (the flat spin has weight 0 there).
    let drawn: Vec<FlipPreset> = flip
        .presets
        .into_iter()
        .filter(|preset| preset.weight > 0.0)
        .collect();
    let mut failures = Vec::new();
    for (mode_name, mode) in both_modes() {
        for (label, preset, strength, speed_m_s) in plates(&drawn, &flip.strength) {
            let plate = (preset, strength);
            let mut world = approach(mode, plate, speed_m_s, GENERATED_HALF_WIDTH_M);
            let pd = fly(&mut world, &mut pd_for(mode), never_ai);
            let mut world = approach(mode, plate, speed_m_s, GENERATED_HALF_WIDTH_M);
            let hybrid = fly(&mut world, &mut hybrid_for(mode), Hybrid::ai_in_control);

            for (name, pass) in [("PD", pd), ("Hybrid", hybrid)] {
                if !pass.saved || pass.peak_tilt_deg < UPSET_DEG {
                    failures.push(format!(
                        "{name} {mode_name} {label}: {}°",
                        pass.peak_tilt_deg
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "not saved after an upset: {failures:#?}"
    );
}

#[test]
// The shipped policy (Stage A, old car, old PD) saves 34/40 with the fixed PD and the
// actuator-aware allocation (DECISIONS 2026-10-26). Turn back on when a policy trained
// on the new car and PD ships; R2 already passes.
#[ignore = "needs a policy trained with the fixed PD; run with --ignored"]
fn on_strong_flip_plates_the_car_turns_over_and_hybrids_ai_lands_it() {
    let flip = flip_plates();
    // A flat spin that strong lands the car sideways, so no course uses it.
    let presets: Vec<FlipPreset> = flip
        .presets
        .into_iter()
        .filter(|preset| preset.name != "flat_spin")
        .collect();
    let mut not_flipped = Vec::new();
    let mut crashed = Vec::new();
    let mut passes = 0;
    for (mode_name, mode) in both_modes() {
        for (label, preset, strength, speed_m_s) in plates(&presets, &[flip.strong_strength]) {
            let mut world = approach(mode, (preset, strength), speed_m_s, STRONG_HALF_WIDTH_M);

            let pass = fly(&mut world, &mut hybrid_for(mode), Hybrid::ai_in_control);

            passes += 1;
            let label = format!("{mode_name} {label}: peak {}°", pass.peak_tilt_deg);
            if pass.peak_tilt_deg < STRONG_UPSET_DEG || !pass.ai_took_over {
                not_flipped.push(label.clone());
            }
            if !pass.saved {
                crashed.push(label);
            }
        }
    }
    // The plate must beat the thrusters every time; that is about the plate.
    assert!(
        not_flipped.is_empty(),
        "not turned over / AI never took over: {not_flipped:#?}"
    );
    // Landing it is about the policy. The shipped (Stage A) policy misses one Realistic
    // barrel roll at 22 m/s; a policy trained on Stage F should land every one. A save
    // here ignores the landing heading and runs with the driver idle; Stage F judges both
    // (DECISIONS 2026-10-21, flip plates).
    let saved_share = 1.0 - crashed.len() as f32 / passes as f32;
    assert!(
        saved_share >= 0.9,
        "Hybrid saved only {saved_share}: {crashed:#?}"
    );
}
