//! The training viewer's crowd (SPEC §11.6, M6, D1): many episodes at once, all flown by
//! one policy (one checkpoint), for the browser to draw side by side. Each car is a full
//! `nl_sim::episode::Episode` stepped by `nl_control::eval::step_episode`, so landings,
//! crashes and time limits follow the same rules as training and eval. Every car is flown
//! by the Hybrid (`build_controller("hybrid", ...)` with controllers.toml's hand-over), as
//! the shipped game and the eval do, with the checkpoint as its AI: PD flies the car
//! until the hand-over tilt, so the viewer shows what the policy adds to it. Built once,
//! the controller shares one parsed policy between every car. `Crowd::for_stage` covers
//! Stage A (flat or in the obstacle arena), Stages D and F on a generated course, and the
//! Gauntlet; the course's shapes are handed to the viewer to draw.

use nl_control::config::DEFAULT_CONTROLLERS_TOML;
use nl_control::eval::step_episode;
use nl_control::{Controller, ControllerKind, build_controller};
use nl_sim::conventions::{NOZZLE_COUNT, WHEEL_COUNT};
use nl_sim::course::{ARENA_JSON, CourseSpec, GAUNTLET_JSON};
use nl_sim::course_build::build_course_shapes;
use nl_sim::episode::{Episode, EpisodeStatus, course_for_stage};
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::randomization::EpisodeVariation;
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig, Stage};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use wasm_bindgen::prelude::*;

use crate::{FLOATS_PER_POSE, flatten_poses};

/// What `outcomes` reports for each car.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum CrowdOutcome {
    /// Still in the air or settling.
    Running = 0,
    /// The episode ended with the car back on its wheels: it succeeded (SPEC §9.5), or
    /// it ran out of time after a save without crashing. The second case keeps the
    /// viewer honest about landings for policies that still blow air while parked,
    /// which the quiet-throttle success rule does not count as a success.
    Landed = 1,
    /// Crashed.
    Crashed = 2,
    /// Ran out of time.
    TimedOut = 3,
}

/// What a crowd flies: a stage and where.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrowdStage {
    /// Stage A drops on flat ground.
    FlatDrop,
    /// Stage B: the car lies on its side on flat ground and must right itself.
    FlatSide,
    /// Stage A drops into the obstacle arena (`courses/arena.json`).
    Arena,
    /// Stage D: the driver bot takes a generated course, all cars the same one.
    Driving,
    /// Stage F: the bot rolls onto a flip plate, all cars on the same generated straight.
    Flight,
    /// Stage D on the Gauntlet (`courses/gauntlet.json`). Historical: the viewer does not show it.
    Gauntlet,
}

impl CrowdStage {
    /// The names `Crowd::for_stage` accepts.
    pub const NAMES: [&'static str; 6] = ["A", "B", "arena", "D", "F", "gauntlet"];

    /// The stage called `name` (see `NAMES`).
    pub fn from_name(name: &str) -> Option<CrowdStage> {
        match name {
            "A" => Some(CrowdStage::FlatDrop),
            "B" => Some(CrowdStage::FlatSide),
            "arena" => Some(CrowdStage::Arena),
            "D" => Some(CrowdStage::Driving),
            "F" => Some(CrowdStage::Flight),
            "gauntlet" => Some(CrowdStage::Gauntlet),
            _ => None,
        }
    }

    /// The training stage the episodes run.
    fn stage(self) -> Stage {
        match self {
            CrowdStage::FlatDrop | CrowdStage::Arena => Stage::A,
            CrowdStage::FlatSide => Stage::B,
            CrowdStage::Driving | CrowdStage::Gauntlet => Stage::D,
            CrowdStage::Flight => Stage::F,
        }
    }
}

/// A crowd of episodes of one stage, flown by the Hybrid with one policy.
#[wasm_bindgen]
pub struct Crowd {
    stage: CrowdStage,
    episodes: Vec<Episode>,
    /// Each car's controller: a clone of one Hybrid, so they share one policy.
    pilots: Vec<ControllerKind>,
    /// Each car's throttles, held between control steps (SPEC §7.1).
    throttles: Vec<[f32; NOZZLE_COUNT]>,
    /// The course's shapes as JSON for the renderer; None on flat ground.
    course_shapes_json: Option<String>,
}

#[wasm_bindgen]
impl Crowd {
    /// `count` cars, car `i` dropped from Stage A seed `first_seed + i` (full difficulty,
    /// no randomization) in `mode` ("realistic" or "arcade") on flat ground, all flown by
    /// the Hybrid with the policy in `policy_json` (a checkpoint file or models/policy.json).
    #[wasm_bindgen(constructor)]
    pub fn new(
        policy_json: &str,
        mode: &str,
        first_seed: u64,
        count: u32,
    ) -> Result<Crowd, JsError> {
        Crowd::for_stage(policy_json, mode, "A", first_seed, count)
    }

    /// `count` cars of the stage called `stage` (see `CrowdStage::NAMES`: "A" flat
    /// drops, "arena" drops into the obstacle arena, "D" and "F" on the generated course
    /// of seed `first_seed`, "gauntlet"), car `i` from seed `first_seed + i`, all flown by
    /// the Hybrid with the policy in `policy_json`. Cars of D and F share the course of
    /// the first seed (so the viewer can draw it once); their seeds vary the driver's
    /// speed and the start. Throws for an unknown stage or mode.
    pub fn for_stage(
        policy_json: &str,
        mode: &str,
        stage: &str,
        first_seed: u64,
        count: u32,
    ) -> Result<Crowd, JsError> {
        let kind = CrowdStage::from_name(stage).ok_or_else(|| {
            JsError::new(&format!(
                "Crowd: stage must be one of {}",
                CrowdStage::NAMES.join(", ")
            ))
        })?;
        Crowd::build(policy_json, mode, kind, first_seed, count, None)
    }

    /// Like the constructor, but every car drops onto the course in `course_json` (the
    /// text of a `courses/*.json` file, SPEC §7.11), all sharing one copy of it. Throws,
    /// naming the piece and field, if the course is invalid.
    pub fn on_course(
        policy_json: &str,
        mode: &str,
        first_seed: u64,
        count: u32,
        course_json: &str,
    ) -> Result<Crowd, JsError> {
        let course = CourseSpec::from_json_str(course_json)?;
        let kind = CrowdStage::Arena;
        Crowd::build(policy_json, mode, kind, first_seed, count, Some(&course))
    }

    /// The controller every car is flown by ("hybrid").
    pub fn controller_name(&self) -> String {
        self.pilots
            .first()
            .map_or("hybrid", ControllerKind::name)
            .to_string()
    }

    /// The course's shapes as JSON for the renderer (the same list `Game.load_course`
    /// returns), or undefined for a crowd on flat ground.
    pub fn course_shapes(&self) -> Option<String> {
        self.course_shapes_json.clone()
    }

    /// How many cars there are.
    pub fn count(&self) -> usize {
        self.episodes.len()
    }

    /// One physics step (1/120 s) for every car still running, the policy acting at
    /// 60 Hz exactly as in eval. Returns false once every car's episode has ended.
    pub fn step(&mut self) -> bool {
        // A finished episode's step does nothing, so every car can simply be stepped.
        let mut any_running = false;
        for index in 0..self.episodes.len() {
            let episode = &mut self.episodes[index];
            let status = step_episode(episode, &mut self.pilots[index], &mut self.throttles[index]);
            any_running |= status == EpisodeStatus::Running;
        }
        any_running
    }

    /// Every car's body pose, `count` × [x, y, z, qx, qy, qz, qw].
    pub fn chassis_poses(&self) -> Vec<f32> {
        let mut values = Vec::with_capacity(self.episodes.len() * FLOATS_PER_POSE);
        for episode in &self.episodes {
            let pose = episode.world().chassis_pose();
            values.extend_from_slice(&pose.position_m);
            values.extend_from_slice(&pose.rotation_xyzw);
        }
        values
    }

    /// Every car's wheel poses, `count` × 4 × [x, y, z, qx, qy, qz, qw].
    pub fn wheel_poses(&self) -> Vec<f32> {
        let size = self.episodes.len() * WHEEL_COUNT * FLOATS_PER_POSE;
        let mut values = Vec::with_capacity(size);
        for episode in &self.episodes {
            values.extend(flatten_poses(&episode.world().wheel_poses()));
        }
        values
    }

    /// Every car's outcome so far, as `CrowdOutcome` numbers.
    pub fn outcomes(&self) -> Vec<u8> {
        let mut codes = Vec::with_capacity(self.episodes.len());
        for episode in &self.episodes {
            let outcome = match episode.status() {
                EpisodeStatus::Running => CrowdOutcome::Running,
                EpisodeStatus::Succeeded => CrowdOutcome::Landed,
                // A turned-over landing or a hit parked car ends the episode: a crash here.
                EpisodeStatus::Crashed(_)
                | EpisodeStatus::HitObstacle
                | EpisodeStatus::BadLanding => CrowdOutcome::Crashed,
                // Only drops count a save followed by a time-out as a landing; a driving
                // stage must reach its finish.
                EpisodeStatus::TimedOut
                    if self.stage.stage() == Stage::A && episode.saves() > 0 =>
                {
                    CrowdOutcome::Landed
                }
                EpisodeStatus::TimedOut => CrowdOutcome::TimedOut,
            };
            codes.push(outcome as u8);
        }
        codes
    }

    /// Every car's episode row so far (the eval `EpisodeResult` fields), as JSON.
    pub fn results_json(&self) -> Result<String, JsError> {
        let mut rows = Vec::with_capacity(self.episodes.len());
        for episode in &self.episodes {
            rows.push(episode.result());
        }
        Ok(serde_json::to_string(&rows)?)
    }
}

impl Crowd {
    /// The cars of `new`, `for_stage` and `on_course`. `course` overrides the stage's own
    /// (the obstacle arena, or any course a Stage A crowd drops onto).
    fn build(
        policy_json: &str,
        mode: &str,
        stage: CrowdStage,
        first_seed: u64,
        count: u32,
        course: Option<&CourseSpec>,
    ) -> Result<Crowd, JsError> {
        let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML)?;
        let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)?
            .by_name(mode)
            .ok_or_else(|| JsError::new("Crowd: mode must be realistic or arcade"))?;
        let scenarios = ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML)?;
        let plain = EpisodeVariation::none(&vehicle, mode);
        // The policy is parsed once here; cloning the Hybrid shares its weights.
        let hybrid = build_controller(
            "hybrid",
            &vehicle,
            mode,
            DEFAULT_CONTROLLERS_TOML,
            Some(policy_json),
        )?;
        let owned_course = match (course, stage) {
            (Some(_), _) => None,
            (None, CrowdStage::Arena) => Some(CourseSpec::from_json_str(ARENA_JSON)?),
            (None, CrowdStage::Gauntlet) => Some(CourseSpec::from_json_str(GAUNTLET_JSON)?),
            (None, CrowdStage::Driving | CrowdStage::Flight) => {
                course_for_stage(&plain, &scenarios, stage.stage(), first_seed)?
            }
            (None, CrowdStage::FlatDrop | CrowdStage::FlatSide) => None,
        };
        let course = course.or(owned_course.as_ref());
        let mut episodes = Vec::new();
        let mut pilots = Vec::new();
        for offset in 0..u64::from(count) {
            let seed = first_seed
                .checked_add(offset)
                .ok_or_else(|| JsError::new("Crowd: first_seed is too large"))?;
            let episode = match course {
                Some(course) => {
                    Episode::start_on_course(&plain, &scenarios, stage.stage(), course, seed)?
                }
                None => Episode::start(&plain, &scenarios, stage.stage(), seed)?,
            };
            episodes.push(episode);
            let mut pilot = hybrid.clone();
            pilot.reset(seed);
            pilots.push(pilot);
        }
        let course_shapes_json = match course {
            Some(course) => Some(serde_json::to_string(&build_course_shapes(course))?),
            None => None,
        };
        let throttles = vec![[0.0; NOZZLE_COUNT]; episodes.len()];
        Ok(Crowd {
            stage,
            episodes,
            pilots,
            throttles,
            course_shapes_json,
        })
    }
}

#[cfg(test)]
mod tests {
    // Test helpers may panic on bad setup; that is a test failure, not a library bug.
    #![allow(clippy::unwrap_used)]

    use nl_control::eval::{run_episode, run_episode_on_course};

    use super::*;

    /// The shipped policy (the viewer's checkpoints share its format).
    const POLICY_JSON: &str = include_str!("../../../models/policy.json");

    fn crowd(stage: &str, first_seed: u64, count: u32) -> Crowd {
        Crowd::for_stage(POLICY_JSON, "realistic", stage, first_seed, count)
            .ok()
            .unwrap()
    }

    /// Steps `crowd` until every car has ended.
    fn run_to_end(crowd: &mut Crowd) {
        let mut steps = 0;
        while crowd.step() {
            steps += 1;
            assert!(steps < 120 * 120, "the crowd never finished");
        }
    }

    #[test]
    fn the_crowd_covers_stage_a_d_f_the_gauntlet_and_the_arena() {
        assert_eq!(CrowdStage::NAMES, ["A", "B", "arena", "D", "F", "gauntlet"]);
        for name in CrowdStage::NAMES {
            let mut crowd = crowd(name, 0, 2);

            for _ in 0..240 {
                crowd.step();
            }

            assert_eq!(crowd.count(), 2, "{name}");
            // Only the flat stages have no course to draw.
            assert_eq!(
                crowd.course_shapes().is_some(),
                !["A", "B"].contains(&name),
                "{name}"
            );
        }
    }

    #[test]
    fn every_car_is_flown_by_the_hybrid() {
        for name in CrowdStage::NAMES {
            assert_eq!(crowd(name, 0, 2).controller_name(), "hybrid", "{name}");
        }
        assert_eq!(
            Crowd::new(POLICY_JSON, "realistic", 0, 1)
                .ok()
                .unwrap()
                .controller_name(),
            "hybrid"
        );
    }

    /// The Hybrid the eval builds for the shipped policy, from the same configs.
    fn eval_hybrid() -> ControllerKind {
        let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
        let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
            .unwrap()
            .realistic;
        build_controller(
            "hybrid",
            &vehicle,
            mode,
            DEFAULT_CONTROLLERS_TOML,
            Some(POLICY_JSON),
        )
        .unwrap()
    }

    /// The plain episode variation and scenarios the crowd uses.
    fn setup() -> (EpisodeVariation, ScenariosConfig) {
        let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
        let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
            .unwrap()
            .realistic;
        let scenarios = ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML).unwrap();
        (EpisodeVariation::none(&vehicle, mode), scenarios)
    }

    #[test]
    fn stage_a_cars_end_exactly_as_the_hybrid_eval_does() {
        let (plain, scenarios) = setup();
        let mut crowd = crowd("A", 0, 6);

        run_to_end(&mut crowd);

        let mut expected = Vec::new();
        for seed in 0..6 {
            let mut hybrid = eval_hybrid();
            expected.push(run_episode(&plain, &scenarios, Stage::A, &mut hybrid, seed).unwrap());
        }
        assert_eq!(
            crowd.results_json().ok().unwrap(),
            serde_json::to_string(&expected).unwrap()
        );
    }

    #[test]
    fn stage_b_cars_end_exactly_as_the_hybrid_eval_does() {
        let (plain, scenarios) = setup();
        let mut crowd = crowd("B", 0, 4);

        run_to_end(&mut crowd);

        let mut expected = Vec::new();
        for seed in 0..4 {
            let mut hybrid = eval_hybrid();
            expected.push(run_episode(&plain, &scenarios, Stage::B, &mut hybrid, seed).unwrap());
        }
        assert_eq!(
            crowd.results_json().ok().unwrap(),
            serde_json::to_string(&expected).unwrap()
        );
    }

    #[test]
    fn the_gauntlet_and_a_generated_course_are_flown_as_the_eval_flies_them() {
        let (plain, scenarios) = setup();
        let gauntlet = CourseSpec::from_json_str(GAUNTLET_JSON).unwrap();
        let generated = course_for_stage(&plain, &scenarios, Stage::D, 4)
            .unwrap()
            .unwrap();
        for (name, stage, course) in [("gauntlet", Stage::D, gauntlet), ("D", Stage::D, generated)]
        {
            let seed = if name == "D" { 4 } else { 0 };
            let mut crowd = crowd(name, seed, 2);

            run_to_end(&mut crowd);

            let mut expected = Vec::new();
            for offset in 0..2 {
                let mut hybrid = eval_hybrid();
                expected.push(
                    run_episode_on_course(
                        &plain,
                        &scenarios,
                        stage,
                        &course,
                        &mut hybrid,
                        seed + offset,
                    )
                    .unwrap(),
                );
            }
            assert_eq!(
                crowd.results_json().ok().unwrap(),
                serde_json::to_string(&expected).unwrap(),
                "{name}"
            );
        }
    }

    #[test]
    fn a_stage_d_crowd_finishes_its_course_and_a_drop_lands() {
        let mut driving = crowd("D", 0, 4);
        let mut drops = crowd("A", 0, 8);

        run_to_end(&mut driving);
        run_to_end(&mut drops);

        let landed = |crowd: &Crowd| {
            crowd
                .outcomes()
                .iter()
                .filter(|&&code| code == CrowdOutcome::Landed as u8)
                .count()
        };
        assert!(
            landed(&driving) >= 3,
            "{} of 4 reached the finish",
            landed(&driving)
        );
        assert!(landed(&drops) >= 4, "{} of 8 drops landed", landed(&drops));
    }
}
