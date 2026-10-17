//! The Gauntlet is playable (SPEC §13 M7): from the start and from every checkpoint,
//! where a crashed car respawns at rest, full throttle with the PD controller reaches
//! the next checkpoint (or the finish) without crashing. This guards the course layout
//! (enough run-up before each jump) and the piece shapes (no ramp foot, kicker or bump
//! that stops or crashes the car). A second test drives the whole course in one go,
//! crashes and respawns included, as the game does.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_control::config::DEFAULT_CONTROLLERS_TOML;
use nl_control::{ControlCtx, Controller, build_controller};
use nl_sim::course::{CourseSpec, GAUNTLET_JSON, PieceSpec};
use nl_sim::events::SimEvent;
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// Longest a section may take from a standing start, in s.
const SECTION_LIMIT_S: usize = 25;
const FULL_THROTTLE: InputFrame = InputFrame {
    throttle: 127,
    steer: 0,
    buttons: 0,
};
const LEVEL: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// The x of the start and of every checkpoint line, in course order.
fn section_starts_m(course: &CourseSpec) -> Vec<f32> {
    let mut starts = vec![course.start.pos[0]];
    for piece in &course.pieces {
        if let PieceSpec::Checkpoint { pos, .. } = piece {
            starts.push(pos[0]);
        }
    }
    starts
}

/// Drives from rest at `start_x_m` with full throttle and PD until the event that ends
/// the section, and returns what stopped it.
fn drive_section(course: &CourseSpec, start_x_m: f32, wanted: SimEvent) -> Result<(), String> {
    let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
    let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
        .unwrap()
        .realistic;
    let mut world = World::new_on_course(&vehicle, mode, course, DEFAULT_GROUND_FRICTION, 0);
    let mut pd = build_controller("pd", &vehicle, mode, DEFAULT_CONTROLLERS_TOML, None).unwrap();
    world
        .place_car([start_x_m, 0.7, 0.0], LEVEL, [0.0; 3], [0.0; 3])
        .unwrap();
    for _ in 0..SECTION_LIMIT_S * 120 {
        if world.is_control_step() {
            let throttles = pd.act(&ControlCtx::for_world(&world, FULL_THROTTLE));
            world.set_controller_throttles(throttles);
        }
        world.step(FULL_THROTTLE);
        for event in world.drain_events() {
            if event == wanted
                || matches!(
                    (event, wanted),
                    (SimEvent::Finished { .. }, SimEvent::Finished { .. })
                )
            {
                return Ok(());
            }
            if let SimEvent::Crash { cause } = event {
                let x_m = world.chassis_pose().position_m[0];
                return Err(format!("crashed ({cause:?}) at x = {x_m:.1} m"));
            }
        }
    }
    let x_m = world.chassis_pose().position_m[0];
    Err(format!("stuck at x = {x_m:.1} m after {SECTION_LIMIT_S} s"))
}

#[test]
fn every_gauntlet_section_can_be_driven_from_a_standing_start() {
    let course = CourseSpec::from_json_str(GAUNTLET_JSON).unwrap();
    let starts = section_starts_m(&course);
    let mut failures = Vec::new();

    for (section, start_x_m) in starts.iter().enumerate() {
        // Section 0 starts at the start line and ends at checkpoint 0; the last ends at
        // the finish.
        let wanted = if section + 1 < starts.len() {
            SimEvent::CheckpointReached {
                number: section as u32,
            }
        } else {
            SimEvent::Finished { time_s: 0.0 }
        };
        if let Err(problem) = drive_section(&course, *start_x_m, wanted) {
            failures.push(format!("from x = {start_x_m} m: {problem}"));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_whole_gauntlet_can_be_finished_in_one_run_with_respawns() {
    // Respawning a car that crashed against a ramp once broke Rapier's island
    // bookkeeping (it panicked on the next step in debug builds).
    let course = CourseSpec::from_json_str(GAUNTLET_JSON).unwrap();
    let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
    let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
        .unwrap()
        .realistic;
    let mut world = World::new_on_course(&vehicle, mode, &course, DEFAULT_GROUND_FRICTION, 0);
    world.enable_auto_respawn(vehicle.events.respawn_delay_s);
    let mut pd = build_controller("pd", &vehicle, mode, DEFAULT_CONTROLLERS_TOML, None).unwrap();

    for _ in 0..90 * 120 {
        if world.is_finished() {
            break;
        }
        if world.is_crashed() {
            pd.reset(0);
        } else if world.is_control_step() {
            let throttles = pd.act(&ControlCtx::for_world(&world, FULL_THROTTLE));
            world.set_controller_throttles(throttles);
        }
        world.step(FULL_THROTTLE);
        world.drain_events();
    }

    let x_m = world.chassis_pose().position_m[0];
    assert!(
        world.is_finished(),
        "not finished after 90 s; car at x = {x_m:.1} m"
    );
    assert_eq!(world.checkpoints_reached(), world.checkpoint_count());
}
