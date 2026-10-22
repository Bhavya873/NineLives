//! How an episode of each stage begins (SPEC §9.5–§9.7): which course it runs on, where
//! the car starts and how fast, what success means, and (for the driving stages) the
//! driver bot, the Chaos knocks and the car rows that must not be touched.
//! `episode::Episode` draws a `StagePlan` once per reset, then applies the rules each step.

use crate::course::{CourseSpec, PieceSpec};
use crate::course_build::{PARKED_CAR_WIDTH_M, car_row_length_m};
use crate::course_gen::generate_course;
use crate::driver_bot::{Centerline, DriverBot, HopTrigger, has_hop_rows, row_hop_triggers};
use crate::driving_stages::{
    HopSetup, draw_chaos_plan, draw_hop_setup, draw_target_speed, hop_course,
};
use crate::flight_stage::{FlightSetup, draw_flight_setup, flight_course};
use crate::hop_profile::HopProfile;
use crate::randomization::EpisodeVariation;
use crate::scenarios::{ScenariosConfig, Stage, start_episode};
use crate::vehicle_config::VehicleConfig;
use crate::world::{PlaceCarError, World};

/// The obstacle zone reaches this much beyond the car's half size on each side, in m.
/// A hit needs the chassis to touch an obstacle collider too, so the zone only says
/// which rows count; it must include a car stopped nose-first against a row.
const OBSTACLE_ZONE_SLACK_M: f32 = 0.2;

/// What success means for an episode (SPEC §9.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Goal {
    /// All four wheels down for `success_hold_s`, unbroken (Stages A and B).
    HoldOnWheels,
    /// A save after the hop, without touching a car row (Stage C).
    SaveAfterHop,
    /// Cross the finish line (Stage D).
    ReachFinish,
    /// Like `HoldOnWheels`, but a landing that turns the car too far from its take-off
    /// heading fails the episode (Stage F).
    LandAndHold,
}

/// Where a car row stands, as a box around the course's centre line that the car's
/// centre is inside whenever any part of the car is over the row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ObstacleZone {
    centerline: Centerline,
    /// From and to, along the centre line, in m.
    along_m: [f32; 2],
    /// From and to, right of the centre line, in m.
    right_m: [f32; 2],
}

impl ObstacleZone {
    /// True when `position_m` (world) is inside the zone.
    pub(crate) fn contains(&self, position_m: [f32; 3]) -> bool {
        let along_m = self.centerline.along_m(position_m);
        let right_m = self.centerline.right_of_line_m(position_m);
        (self.along_m[0]..=self.along_m[1]).contains(&along_m)
            && (self.right_m[0]..=self.right_m[1]).contains(&right_m)
    }
}

/// Everything an episode's rules need beyond the world.
pub(crate) struct StageStart {
    /// What success means.
    pub(crate) goal: Goal,
    /// Stage E on A or C: success also needs four wheels down for the hold time after
    /// the last knock.
    pub(crate) hold_after_knocks: bool,
    /// The driver (none on Stages A and B).
    pub(crate) driver: Option<DriverBot>,
    /// Physics steps (from the start) on which Chaos knocks, sorted.
    pub(crate) knock_steps: Vec<u64>,
    /// Car rows the car must not touch (Stage C).
    pub(crate) obstacles: Vec<ObstacleZone>,
}

/// One episode's draws, made once per reset and shared by everything that sets it up.
pub(crate) struct StagePlan {
    /// The stage asked for (its time limit applies).
    pub(crate) stage: Stage,
    /// The stage actually run: `stage`, or for Stage E the A, C or D it builds on.
    pub(crate) base: Stage,
    /// Physics steps of the Chaos knocks (Stage E only).
    knock_steps: Vec<u64>,
    /// Stage C's draws and the car's hop prediction they were made with.
    hop: Option<(HopSetup, HopProfile)>,
    /// Stage F's draws.
    flight: Option<FlightSetup>,
    /// The episode's time limit, in s.
    time_limit_s: f32,
}

impl StagePlan {
    /// The draws for an episode of `stage` from `seed`. Stage F needs the flip plates
    /// from pieces.json.
    pub(crate) fn draw(
        variation: &EpisodeVariation,
        scenarios: &ScenariosConfig,
        stage: Stage,
        seed: u64,
    ) -> Result<StagePlan, PlaceCarError> {
        let mut base = stage;
        let mut knock_steps = Vec::new();
        if stage == Stage::E {
            let chaos = draw_chaos_plan(&scenarios.stage_e, seed);
            base = chaos.base_stage;
            knock_steps = chaos.knock_steps;
        }
        let mut hop = None;
        if base == Stage::C {
            let profile = HopProfile::new(&variation.vehicle, &variation.mode);
            let clearance_m = scenarios.driver_bot.hop_clearance_m;
            let vehicle = &variation.vehicle;
            let setup = draw_hop_setup(&scenarios.stage_c, clearance_m, vehicle, &profile, seed);
            hop = Some((setup, profile));
        }
        let mut flight = None;
        if stage == Stage::F {
            let plates = scenarios.piece_catalog()?.flip_plates().ok_or_else(|| {
                PlaceCarError::BadCourse("pieces.json: Stage F needs a flip_plate".to_string())
            })?;
            flight = Some(draw_flight_setup(&scenarios.stage_f, &plates, seed));
        }
        Ok(StagePlan {
            stage,
            base,
            knock_steps,
            hop,
            flight,
            time_limit_s: scenarios.time_limit_s(stage),
        })
    }

    /// The episode's time limit, in s.
    pub(crate) fn time_limit_s(&self) -> f32 {
        self.time_limit_s
    }

    /// The course this episode runs on when no course is given: None (flat ground) for
    /// A and B, the Stage C track, the Stage F straight, or a generated Stage D course.
    pub(crate) fn course(
        &self,
        scenarios: &ScenariosConfig,
        seed: u64,
    ) -> Result<Option<CourseSpec>, PlaceCarError> {
        if let Some(setup) = &self.flight {
            let catalog = scenarios.piece_catalog()?;
            return Ok(Some(flight_course(&scenarios.stage_f, catalog, setup)));
        }
        match (self.base, &self.hop) {
            (Stage::A | Stage::B, _) => Ok(None),
            (_, Some((setup, _))) => {
                let catalog = scenarios.piece_catalog()?;
                let config = &scenarios.stage_c;
                Ok(Some(hop_course(config, catalog, setup, self.time_limit_s)))
            }
            _ => Ok(Some(generate_course(scenarios.piece_catalog()?, seed))),
        }
    }
}

/// Places the car in `world` for `plan` from `seed` and sets up the episode's rules.
/// `course` is the course `world` was built on (None: flat ground, which the driving
/// stages treat as an empty straight track along +X).
pub(crate) fn begin_stage(
    world: &mut World,
    variation: &EpisodeVariation,
    scenarios: &ScenariosConfig,
    plan: &StagePlan,
    course: Option<&CourseSpec>,
    seed: u64,
) -> Result<StageStart, PlaceCarError> {
    let mut start = match (plan.base, &plan.hop) {
        (Stage::F, _) => begin_flight(world, variation, scenarios, plan, course, seed)?,
        (Stage::A | Stage::B, _) => {
            start_episode(world, plan.base, scenarios, seed)?;
            no_driver(Goal::HoldOnWheels)
        }
        (_, Some((setup, profile))) => {
            begin_hop(world, variation, scenarios, setup, profile, course, seed)?
        }
        _ => begin_course(world, variation, scenarios, plan, course, seed)?,
    };
    start.hold_after_knocks = plan.stage == Stage::E && start.goal != Goal::ReachFinish;
    start.knock_steps = plan.knock_steps.clone();
    Ok(start)
}

/// Stage C: the car rolls at the drawn speed and the bot holds it, pressing Hop before
/// each car row marked `hop_hint`, or at the drawn time if there is none.
fn begin_hop(
    world: &mut World,
    variation: &EpisodeVariation,
    scenarios: &ScenariosConfig,
    setup: &HopSetup,
    profile: &HopProfile,
    course: Option<&CourseSpec>,
    seed: u64,
) -> Result<StageStart, PlaceCarError> {
    let bot_config = scenarios.driver_bot;
    let vehicle = &variation.vehicle;
    let centerline = course.map_or(Centerline::along_x(), Centerline::of_course);
    let mut triggers = Vec::new();
    let mut obstacles = Vec::new();
    if let Some(course) = course {
        triggers = row_hop_triggers(course, &centerline, profile, bot_config.hop_clearance_m);
        obstacles = obstacle_zones(course, &centerline, vehicle);
    }
    if triggers.is_empty() {
        triggers.push(HopTrigger::AtTime {
            time_s: setup.hop_time_s,
        });
    }
    roll_forward(world, setup.speed_m_s)?;
    let bot = DriverBot::new(
        bot_config,
        vehicle,
        centerline,
        setup.speed_m_s,
        &triggers,
        seed,
    );
    Ok(StageStart {
        obstacles,
        ..with_driver(Goal::SaveAfterHop, bot)
    })
}

/// Stage D: the car starts at rest and the bot drives to the finish at a drawn target
/// speed, raised if needed so the course fits in the running stage's time limit.
fn begin_course(
    world: &mut World,
    variation: &EpisodeVariation,
    scenarios: &ScenariosConfig,
    plan: &StagePlan,
    course: Option<&CourseSpec>,
    seed: u64,
) -> Result<StageStart, PlaceCarError> {
    let bot_config = scenarios.driver_bot;
    let vehicle = &variation.vehicle;
    let centerline = course.map_or(Centerline::along_x(), Centerline::of_course);
    let mut triggers = Vec::new();
    let mut needed_m_s = 0.0;
    if let Some(course) = course {
        if has_hop_rows(course) {
            let profile = HopProfile::new(vehicle, &variation.mode);
            let clearance_m = bot_config.hop_clearance_m;
            triggers = row_hop_triggers(course, &centerline, &profile, clearance_m);
        }
        let usable_s = scenarios.stage_d.time_use_fraction * plan.time_limit_s;
        needed_m_s = finish_distance_m(course, &centerline) / usable_s;
    }
    let target_m_s = draw_target_speed(bot_config.target_speed_m_s, needed_m_s, seed);
    roll_forward(world, 0.0)?;
    let bot = DriverBot::new(bot_config, vehicle, centerline, target_m_s, &triggers, seed);
    Ok(with_driver(Goal::ReachFinish, bot))
}

/// Stage F: the car rolls at the drawn speed towards the plate and the bot holds that
/// speed along the course, so the driver's throttle and steer stay live through the flight.
fn begin_flight(
    world: &mut World,
    variation: &EpisodeVariation,
    scenarios: &ScenariosConfig,
    plan: &StagePlan,
    course: Option<&CourseSpec>,
    seed: u64,
) -> Result<StageStart, PlaceCarError> {
    let speed_m_s = plan.flight.as_ref().map_or(0.0, |setup| setup.speed_m_s);
    let centerline = course.map_or(Centerline::along_x(), Centerline::of_course);
    roll_forward(world, speed_m_s)?;
    let bot_config = scenarios.driver_bot;
    let bot = DriverBot::new(
        bot_config,
        &variation.vehicle,
        centerline,
        speed_m_s,
        &[],
        seed,
    );
    Ok(with_driver(Goal::LandAndHold, bot))
}

/// Keeps the car where the world put it (the course start), level and facing the
/// start heading, moving forward at `speed_m_s`, with a full tank.
fn roll_forward(world: &mut World, speed_m_s: f32) -> Result<(), PlaceCarError> {
    let pose = world.chassis_pose();
    let rotation = pose.to_rapier().rotation;
    let velocity = rotation * rapier3d::prelude::Vector::X * speed_m_s;
    world.place_car(
        pose.position_m,
        pose.rotation_xyzw,
        velocity.to_array(),
        [0.0; 3],
    )?;
    world.refill_tank();
    Ok(())
}

/// How far along the centre line the course's finish line is (0 without one), in m.
fn finish_distance_m(course: &CourseSpec, centerline: &Centerline) -> f32 {
    let mut distance_m: f32 = 0.0;
    for piece in &course.pieces {
        if let PieceSpec::Finish { pos, .. } = piece {
            distance_m = distance_m.max(centerline.along_m(*pos));
        }
    }
    distance_m
}

/// A zone around every car row of `course` (rows run along the course).
fn obstacle_zones(
    course: &CourseSpec,
    centerline: &Centerline,
    vehicle: &VehicleConfig,
) -> Vec<ObstacleZone> {
    let [half_length_m, _, half_width_m] = vehicle.chassis.half_extents_m;
    let reach_along_m = half_length_m + OBSTACLE_ZONE_SLACK_M;
    let reach_across_m = PARKED_CAR_WIDTH_M / 2.0 + half_width_m + OBSTACLE_ZONE_SLACK_M;
    let mut zones = Vec::new();
    for piece in &course.pieces {
        if let PieceSpec::CarRow { pos, count, .. } = piece {
            let start_m = centerline.along_m(*pos);
            let end_m = start_m + car_row_length_m(*count);
            let right_m = centerline.right_of_line_m(*pos);
            zones.push(ObstacleZone {
                centerline: *centerline,
                along_m: [start_m - reach_along_m, end_m + reach_along_m],
                right_m: [right_m - reach_across_m, right_m + reach_across_m],
            });
        }
    }
    zones
}

/// A start with no driver, knocks or obstacles.
fn no_driver(goal: Goal) -> StageStart {
    StageStart {
        goal,
        hold_after_knocks: false,
        driver: None,
        knock_steps: Vec::new(),
        obstacles: Vec::new(),
    }
}

/// A start driven by `bot`, with no knocks or obstacles.
fn with_driver(goal: Goal, bot: DriverBot) -> StageStart {
    StageStart {
        driver: Some(bot),
        ..no_driver(goal)
    }
}
