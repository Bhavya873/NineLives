//! Flip plates (SPEC §7.11), with the thrusters off. At generated-course strength every
//! preset crashes the car on most passes; at strong strength every
//! preset but the flat spin turns it upside down on every pass. The hand-made courses'
//! flip plates are exactly presets from configs/pieces.json.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{LEVEL, realistic, vehicle};
use nl_sim::conventions::{INVERTED_TILT_DEG, PHYSICS_HZ};
use nl_sim::course::{CourseSpec, PieceSpec};
use nl_sim::course_gen::{FlipPlates, FlipPreset, PieceCatalog};
use nl_sim::events::SimEvent;
use nl_sim::input::InputFrame;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// Where the plate sits along the course, in m; the car starts 3 m before it.
const PLATE_X_M: f32 = 30.0;
/// How long a flight is watched after the start, in s.
const WATCH_S: f32 = 4.0;
/// Speeds a car meets a plate at, in m/s.
const SPEEDS_M_S: [f32; 6] = [10.0, 13.0, 16.0, 19.0, 22.0, 25.0];
/// "Most passes": where the car meets the ground mid-turn depends on the speed, so a
/// few mild passes land early; the rest must flip.
const MOST_PASSES: f32 = 0.7;
/// Half the width of a generated course's bounds, in m (pieces.json track_half_width_m).
const TRACK_HALF_WIDTH_M: f32 = 10.0;

/// What one unaided pass over a plate did.
struct Flight {
    peak_tilt_deg: f32,
    /// Largest yaw rate about world +Y, in rad/s.
    peak_yaw_rate_rad_s: f32,
    /// Body pitch rate (about body +Z, nose up) just after the kick, in rad/s.
    pitch_rate_after_kick_rad_s: f32,
    crashed: bool,
}

fn flip_plates() -> FlipPlates {
    PieceCatalog::built_in().unwrap().flip_plates().unwrap()
}

fn preset(name: &str) -> FlipPreset {
    let presets = flip_plates().presets;
    presets
        .into_iter()
        .find(|preset| preset.name == name)
        .unwrap()
}

/// A straight course as wide as a generated one, with only `preset`'s plate.
fn plate_world(preset: &FlipPreset, strength: f32) -> World {
    let empty = CourseSpec::from_json_str(&format!(
        r#"{{"version": 1, "name": "Flip test", "start": {{"pos": [0, 0.7, 0]}},
           "bounds": {{"min": [-20, -20, -{TRACK_HALF_WIDTH_M}], "max": [200, 80, {TRACK_HALF_WIDTH_M}]}},
           "pieces": []}}"#
    ))
    .unwrap();
    let spec = CourseSpec {
        pieces: vec![preset.kick_plate_at([PLATE_X_M, 0.0, 0.0], strength)],
        ..empty
    };
    World::new_on_course(&vehicle(), realistic(), &spec, DEFAULT_GROUND_FRICTION, 0)
}

/// Rolls the car over the plate at `speed_m_s` with the thrusters off and watches it.
fn fly_unaided(preset: &FlipPreset, strength: f32, speed_m_s: f32) -> Flight {
    let mut world = plate_world(preset, strength);
    let start_m = [PLATE_X_M - 3.0, 0.7, 0.0];
    world
        .place_car(start_m, LEVEL, [speed_m_s, 0.0, 0.0], [0.0; 3])
        .unwrap();
    let mut flight = Flight {
        peak_tilt_deg: 0.0,
        peak_yaw_rate_rad_s: 0.0,
        pitch_rate_after_kick_rad_s: 0.0,
        crashed: false,
    };
    let mut kicked = false;
    for _ in 0..(WATCH_S * PHYSICS_HZ as f32) as usize {
        world.step(InputFrame::default());
        for event in world.drain_events() {
            match event {
                SimEvent::Kicked { .. } => kicked = true,
                SimEvent::Crash { .. } => flight.crashed = true,
                _ => {}
            }
        }
        if flight.crashed {
            break;
        }
        if kicked && flight.pitch_rate_after_kick_rad_s == 0.0 {
            flight.pitch_rate_after_kick_rad_s = world.angular_velocity_body_rad_s()[2];
        }
        flight.peak_tilt_deg = flight.peak_tilt_deg.max(world.tilt_rad().to_degrees());
        let yaw_rate_rad_s = world.angular_velocity_rad_s()[1].abs();
        flight.peak_yaw_rate_rad_s = flight.peak_yaw_rate_rad_s.max(yaw_rate_rad_s);
    }
    assert!(kicked, "{}: the plate never fired", preset.name);
    flight
}

/// Flies `preset` at each strength in `strengths` and every speed.
fn flights(preset: &FlipPreset, strengths: &[f32]) -> Vec<Flight> {
    let mut flights = Vec::new();
    for strength in strengths {
        for speed_m_s in SPEEDS_M_S {
            flights.push(fly_unaided(preset, *strength, speed_m_s));
        }
    }
    flights
}

/// The share of `flights` (0..1) for which `happened` is true.
fn share(flights: &[Flight], happened: impl Fn(&Flight) -> bool) -> f32 {
    let count = flights.iter().filter(|flight| happened(flight)).count();
    count as f32 / flights.len() as f32
}

#[test]
fn at_course_strength_every_preset_crashes_the_car_on_most_passes() {
    let plates = flip_plates();
    let [weakest, strongest] = plates.strength;
    for preset in &plates.presets {
        let flights = flights(preset, &[weakest, (weakest + strongest) / 2.0, strongest]);

        let crashed = share(&flights, |flight| flight.crashed);

        let name = &preset.name;
        assert!(crashed >= MOST_PASSES, "{name}: only {crashed} crashed");
        if name == "flat_spin" {
            // A flat spin turns the car about its yaw axis rather than over.
            let spinning = share(&flights, |flight| flight.peak_yaw_rate_rad_s >= 3.0);
            assert_eq!(
                spinning, 1.0,
                "flat spin: only {spinning} spun past 3 rad/s"
            );
        }
    }
}

#[test]
fn at_strong_strength_every_preset_but_the_flat_spin_turns_the_car_upside_down() {
    let plates = flip_plates();
    for preset in plates
        .presets
        .iter()
        .filter(|preset| preset.name != "flat_spin")
    {
        let flights = flights(preset, &[plates.strong_strength]);

        let inverted = share(&flights, |flight| flight.peak_tilt_deg > INVERTED_TILT_DEG);

        assert_eq!(inverted, 1.0, "{}: only {inverted} inverted", preset.name);
    }
}

#[test]
fn the_front_flip_pitches_the_nose_down_and_the_back_flip_pitches_it_up() {
    let front = fly_unaided(&preset("front_flip"), 1.0, SPEEDS_M_S[1]);
    let back = fly_unaided(&preset("back_flip"), 1.0, SPEEDS_M_S[1]);

    assert!(front.pitch_rate_after_kick_rad_s < -1.0, "front flip");
    assert!(back.pitch_rate_after_kick_rad_s > 1.0, "back flip");
}

/// True when `piece` is `preset` at `strength`, within rounding.
fn is_preset_at(piece: &PieceSpec, preset: &FlipPreset, strength: f32) -> bool {
    let PieceSpec::KickPlate {
        impulse, angular, ..
    } = piece
    else {
        return false;
    };
    let wanted_pairs = impulse
        .iter()
        .zip(preset.impulse_n_s)
        .chain(angular.iter().zip(preset.angular_n_m_s));
    wanted_pairs
        .into_iter()
        .all(|(have, want)| (have - want * strength).abs() < 1.0)
}

#[test]
fn every_flip_plate_on_the_hand_made_courses_is_a_preset_at_strength_1_or_strong() {
    let plates = flip_plates();
    let strengths = [1.0, plates.strong_strength];
    for file in ["gauntlet", "playground"] {
        let path = format!("{}/../../courses/{file}.json", env!("CARGO_MANIFEST_DIR"));
        let course = CourseSpec::from_json_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for piece in &course.pieces {
            // Plain kick plates push up at most 4000 N·s; the weakest flip preset 6500.
            let PieceSpec::KickPlate { impulse, pos, .. } = piece else {
                continue;
            };
            if impulse[1] < 6000.0 {
                continue;
            }
            let matched = plates.presets.iter().any(|preset| {
                strengths
                    .iter()
                    .any(|strength| is_preset_at(piece, preset, *strength))
            });
            assert!(matched, "{file}: kick plate at {pos:?} is no preset");
        }
    }
}
