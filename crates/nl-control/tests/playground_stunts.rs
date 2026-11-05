//! The steep playground ramps (courses/playground.json) throw the car high enough for a
//! planned back flip or barrel roll: the stunt authorizes (the forecast airtime covers the
//! plan's `required_s`), completes the full rotation and lands without a crash. It also
//! checks the steeper ramps do not crash a clean, level approach under PD more often than
//! the old 8 degree playground ramps did.
//!
//! The player is modelled as pressing the stunt key at the first moment the planner would
//! say yes. A ramp launches the car nose up, so the planner's tilt rule makes it wait while
//! PD levels the car; the table shows that wait as "waited".
//!
//! cargo test --release -p nl-control --test playground_stunts -- --ignored --nocapture
//! prints the airtime table (the research note docs/research/playground-ramps.md has it).

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};
use nl_control::stunt::{
    PlannerInputs, StuntDirector, StuntEvent, StuntKind, StuntPhase, authorize,
};
use nl_control::{ControlCtx, Controller, Pd};
use nl_sim::conventions::PHYSICS_DT_S;
use nl_sim::course::{CourseSpec, PLAYGROUND_JSON, PieceSpec};
use nl_sim::events::SimEvent;
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModeParams, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// The car starts this far before the ramp foot, in m, and holds its speed on the way.
const APPROACH_M: f32 = 30.0;
/// Approach speeds tried, in m/s.
const SPEEDS_M_S: [f32; 5] = [20.0, 25.0, 30.0, 35.0, 40.0];
/// The crash comparison leaves out the fastest approach: at 40 m/s a 22 degree ramp throws
/// the car 175 m and the plain PD landing is a hard impact (the table prints it).
const CLEAN_SPEEDS_M_S: [f32; 4] = [20.0, 25.0, 30.0, 35.0];
/// A press at once, and a press once PD has levelled the car within this tilt, in degrees.
const ANY_TILT_DEG: f32 = 90.0;
const LEVEL_TILT_DEG: f32 = 5.0;
/// Throttle per m/s of speed error for the cruise control.
const CRUISE_GAIN_PER_M_S: f32 = 0.5;
/// A run ends this long after touchdown, in s, or at this time, whichever is first.
const AFTER_LANDING_S: f32 = 3.0;
const RUN_LIMIT_S: f32 = 20.0;
/// The old playground's ramps: 15.4 m long, 8 degrees.
const OLD_RAMP_JSON: &str = r#"{"version": 1, "name": "old", "start": {"pos": [0, 0.7, 0], "yaw": 0},
    "bounds": {"min": [-800, -20, -800], "max": [800, 60, 800]},
    "pieces": [{"type": "ramp", "pos": [60, 0, 0], "length": 15.4, "width": 8, "angle_deg": 8}]}"#;

/// What one ramp run showed.
#[derive(Default)]
struct RunResult {
    /// From take-off to touchdown, in s.
    airtime_s: f32,
    crashed: bool,
    /// The planner's first yes: (required_s, forecast landing time, wait after take-off), s.
    plan: Option<(f32, f32, f32)>,
    /// The planner counted a full rotation at some point.
    rotation_done: bool,
    /// The planner reported `Landed` (a full rotation still counted after PD's catch).
    landed_event: bool,
    wheels_down_at_end: usize,
    /// The planner's own events, for failure messages.
    events: Vec<StuntEvent>,
}

impl RunResult {
    /// A stunt that authorized, turned all the way round and came down on four wheels.
    fn stunt_landed(&self) -> bool {
        self.plan.is_some() && self.rotation_done && !self.crashed && self.wheels_down_at_end == 4
    }
}

fn mode() -> ModeParams {
    ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
        .unwrap()
        .realistic
}

/// Drives the ramp `piece` of `course` at `speed_m_s`; with a `stunt`, the player presses
/// it once the planner allows (and the car is within the given tilt of level), and the stunt director flies it with PD as the fallback.
fn run(
    course: &CourseSpec,
    piece: usize,
    speed_m_s: f32,
    stunt: Option<(StuntKind, f32)>,
) -> RunResult {
    let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
    let controllers = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();
    let mode = mode();
    let mut world = World::new_on_course(&vehicle, mode, course, DEFAULT_GROUND_FRICTION, 0);
    let (foot, yaw_deg) = match &course.pieces[piece] {
        PieceSpec::Ramp { pos, yaw, .. } => (*pos, *yaw),
        other => panic!("not a ramp: {other:?}"),
    };
    let yaw = yaw_deg.to_radians();
    let heading = [yaw.cos(), 0.0, -yaw.sin()];
    let start = [
        foot[0] - heading[0] * APPROACH_M,
        0.7,
        foot[2] - heading[2] * APPROACH_M,
    ];
    let rotation = [0.0, (yaw / 2.0).sin(), 0.0, (yaw / 2.0).cos()];
    world
        .place_car(start, rotation, heading.map(|c| c * speed_m_s), [0.0; 3])
        .unwrap();

    let pd = Pd::for_vehicle(&vehicle, mode, &controllers);
    let mut fallback = pd.clone();
    let mut director = StuntDirector::new(controllers.stunt, pd);
    let mut result = RunResult::default();
    let mut takeoff_s: Option<f32> = None;
    let mut landing_s: Option<f32> = None;
    let mut time_s = 0.0;
    while time_s < RUN_LIMIT_S && !result.crashed {
        let error = speed_m_s - world.forward_speed_m_s();
        let input = InputFrame {
            throttle: ((error * CRUISE_GAIN_PER_M_S).clamp(-1.0, 1.0) * 127.0) as i8,
            ..InputFrame::default()
        };
        if world.is_control_step() {
            let ctx = ControlCtx::for_world(&world, input);
            let step = director.step(&world, &ctx, 1.0 / 60.0);
            result.events.extend(step.tick.events.iter().copied());
            result.rotation_done |= director.planner().rotation_complete()
                && director.planner().phase() != StuntPhase::Idle;
            if let (Some((kind, max_tilt_deg)), Some(takeoff), None, None) =
                (stunt, takeoff_s, landing_s, result.plan)
            {
                let config = *director.planner().config();
                let inputs = PlannerInputs::from_world(&world, &config, 1.0 / 60.0);
                let tilt_deg = world.tilt_rad().to_degrees();
                if let (true, Ok(plan)) =
                    (tilt_deg <= max_tilt_deg, authorize(&config, kind, &inputs))
                {
                    let forecast_s = inputs.forecast.map_or(0.0, |forecast| forecast.time_s);
                    result.plan = Some((plan.required_s, forecast_s, time_s - takeoff));
                    director.request(kind, world.time_s() as f32);
                }
            }
            let throttles = step.throttles.unwrap_or_else(|| fallback.act(&ctx));
            world.set_controller_throttles(throttles);
        }
        world.step(input);
        time_s += PHYSICS_DT_S;
        let position = world.chassis_pose().position_m;
        let along_m = (position[0] - start[0]) * heading[0] + (position[2] - start[2]) * heading[2];
        for event in world.drain_events() {
            match event {
                SimEvent::AirborneStart if along_m > APPROACH_M && takeoff_s.is_none() => {
                    takeoff_s = Some(time_s);
                }
                SimEvent::AirborneEnd if takeoff_s.is_some() && landing_s.is_none() => {
                    landing_s = Some(time_s);
                    result.airtime_s = time_s - takeoff_s.unwrap_or(time_s);
                }
                SimEvent::Crash { .. } => result.crashed = true,
                _ => {}
            }
        }
        if landing_s.is_some_and(|landed| time_s - landed >= AFTER_LANDING_S) {
            break;
        }
    }
    result.landed_event = result
        .events
        .iter()
        .any(|event| matches!(event, StuntEvent::Landed { .. }));
    result.wheels_down_at_end = world.wheels_touching_count();
    result
}

/// The playground's ramps, one per distinct angle: (piece index, angle in degrees).
fn playground_ramps(course: &CourseSpec) -> Vec<(usize, f32)> {
    let mut ramps: Vec<(usize, f32)> = Vec::new();
    for (index, piece) in course.pieces.iter().enumerate() {
        if let PieceSpec::Ramp { angle_deg, .. } = piece {
            if ramps.iter().all(|(_, angle)| angle != angle_deg) {
                ramps.push((index, *angle_deg));
            }
        }
    }
    ramps.sort_by(|a, b| a.1.total_cmp(&b.1));
    ramps
}

fn describe(result: &RunResult) -> String {
    let Some((required_s, forecast_s, waited_s)) = result.plan else {
        return "  no yes".to_string();
    };
    let outcome = if result.stunt_landed() {
        if result.landed_event {
            "landed"
        } else {
            "landed*"
        }
    } else {
        "FAILED"
    };
    format!("{required_s:>5.2} {forecast_s:>5.2} {waited_s:>5.2}  {outcome}")
}

#[test]
#[ignore = "prints the airtime table; slow in debug builds"]
fn airtime_table_for_every_playground_ramp_angle() {
    let course = CourseSpec::from_json_str(PLAYGROUND_JSON).unwrap();
    println!(
        "\nangle speed | airtime s | back flip: required forecast waited result | barrel roll: same"
    );
    println!(
        "(landed* = full turn done and on four wheels, but PD's catch undid part of the counted turn)"
    );
    for (piece, angle_deg) in playground_ramps(&course) {
        for speed_m_s in SPEEDS_M_S {
            let plain = run(&course, piece, speed_m_s, None);
            let flip = run(
                &course,
                piece,
                speed_m_s,
                Some((StuntKind::BackFlip, ANY_TILT_DEG)),
            );
            let level = run(
                &course,
                piece,
                speed_m_s,
                Some((StuntKind::BackFlip, LEVEL_TILT_DEG)),
            );
            let roll = run(
                &course,
                piece,
                speed_m_s,
                Some((StuntKind::BarrelRight, ANY_TILT_DEG)),
            );
            println!(
                "{angle_deg:>4.0} {speed_m_s:>5.0} | {:>9.2}{} | {} | {} | {}",
                plain.airtime_s,
                if plain.crashed { " CRASH" } else { "" },
                describe(&flip),
                describe(&level),
                describe(&roll)
            );
        }
    }
}

#[test]
fn back_flips_and_barrel_rolls_authorize_and_land_on_the_steep_ramps() {
    let course = CourseSpec::from_json_str(PLAYGROUND_JSON).unwrap();
    let ramps = playground_ramps(&course);
    // (ramp angle in degrees, speed in m/s, stunt, press tilt limit in degrees). A barrel
    // roll can be pressed at once; a back flip is pressed once PD has levelled the car,
    // otherwise the catch undoes part of the counted turn and no `Landed` is reported.
    let cases = [
        (24.0, 30.0, StuntKind::BarrelRight, ANY_TILT_DEG),
        (28.0, 25.0, StuntKind::BarrelRight, ANY_TILT_DEG),
        (28.0, 30.0, StuntKind::BarrelLeft, ANY_TILT_DEG),
        (26.0, 30.0, StuntKind::BackFlip, LEVEL_TILT_DEG),
        (28.0, 30.0, StuntKind::BackFlip, LEVEL_TILT_DEG),
        (28.0, 35.0, StuntKind::BackFlip, LEVEL_TILT_DEG),
    ];
    for (angle_deg, speed_m_s, kind, max_tilt_deg) in cases {
        let piece = ramps.iter().find(|ramp| ramp.1 == angle_deg).unwrap().0;
        let result = run(&course, piece, speed_m_s, Some((kind, max_tilt_deg)));
        let label = format!("{kind:?} on {angle_deg} deg at {speed_m_s} m/s");
        let (required_s, forecast_s, _) = result
            .plan
            .unwrap_or_else(|| panic!("{label} never authorized"));
        assert!(
            forecast_s >= required_s,
            "{label}: {forecast_s} < {required_s}"
        );
        assert!(
            result.airtime_s >= required_s,
            "{label}: airtime {} < required {required_s}",
            result.airtime_s
        );
        assert!(result.stunt_landed(), "{label}: {:?}", result.events);
        assert!(result.landed_event, "{label}: {:?}", result.events);
    }
}

#[test]
fn the_steep_ramps_crash_a_clean_approach_no_more_than_the_old_ramps_did() {
    let course = CourseSpec::from_json_str(PLAYGROUND_JSON).unwrap();
    let old = CourseSpec::from_json_str(OLD_RAMP_JSON).unwrap();
    let crashes = |course: &CourseSpec, ramps: &[(usize, f32)]| {
        let mut crashed = 0;
        let mut total = 0;
        for (piece, _) in ramps {
            for speed_m_s in CLEAN_SPEEDS_M_S {
                total += 1;
                crashed += usize::from(run(course, *piece, speed_m_s, None).crashed);
            }
        }
        (crashed, total)
    };
    let (old_crashed, old_total) = crashes(&old, &[(0, 8.0)]);
    let (new_crashed, new_total) = crashes(&course, &playground_ramps(&course));
    println!("old ramps: {old_crashed}/{old_total} crashed; new ramps: {new_crashed}/{new_total}");
    let old_rate = old_crashed as f32 / old_total as f32;
    let new_rate = new_crashed as f32 / new_total as f32;
    assert!(new_rate <= old_rate + 1e-6, "{new_rate} vs old {old_rate}");
}
