//! Episode rules for the driving stages (SPEC §9.5, §9.6), with the controller off so
//! only the rules and the driver bot act: Stage C needs a save after the hop and no
//! touch of the car row, Stage D needs the finish, Stage E needs every knock first, and
//! each stage has its own time limit. Also Stage C's row placement and the
//! `start_on_course` entry point.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{arcade, realistic, scenarios, vehicle};
use nl_sim::conventions::{NOZZLE_COUNT, PHYSICS_HZ};
use nl_sim::course::{BoundsSpec, CourseSpec, GAUNTLET_JSON, PieceSpec, StartSpec};
use nl_sim::driving_stages::{draw_chaos_plan, draw_hop_setup};
use nl_sim::episode::{Episode, EpisodeStatus};
use nl_sim::hop_profile::HopProfile;
use nl_sim::modes::ModeParams;
use nl_sim::randomization::EpisodeVariation;
use nl_sim::scenarios::Stage;

const NO_THRUST: [f32; NOZZLE_COUNT] = [0.0; NOZZLE_COUNT];

fn start(stage: Stage, mode: ModeParams, seed: u64) -> Episode {
    let variation = EpisodeVariation::none(&vehicle(), mode);
    Episode::start(&variation, &scenarios(), stage, seed).unwrap()
}

fn start_on(course: &CourseSpec, stage: Stage, seed: u64) -> Episode {
    let variation = EpisodeVariation::none(&vehicle(), realistic());
    Episode::start_on_course(&variation, &scenarios(), stage, course, seed).unwrap()
}

fn run_to_end(episode: &mut Episode) -> EpisodeStatus {
    let mut status = EpisodeStatus::Running;
    while status == EpisodeStatus::Running {
        status = episode.step(NO_THRUST);
    }
    status
}

/// A flat straight course along +X with `pieces`, 400 m long.
fn straight(pieces: Vec<PieceSpec>) -> CourseSpec {
    CourseSpec {
        name: "test".to_string(),
        start: StartSpec {
            pos: [0.0, 0.7, 0.0],
            yaw: 0.0,
        },
        bounds: BoundsSpec {
            min: [-30.0, -20.0, -10.0],
            max: [400.0, 60.0, 10.0],
        },
        pieces,
    }
}

/// A Stage C seed whose hop lands on its wheels with the thrusters off. Most seeds
/// crash without a controller, so this one is picked by hand; re-pick it (and say so
/// in the PR) when the car's driving numbers change. Re-picked (14 → 87) when air drag got
/// per-axis areas and a centre of pressure behind the centre of mass, which pitches a hop;
/// re-picked again (87 → 49, the only one of seeds 0-119) for the fast roadster drive (G2).
const STAGE_C_LUCKY_SEED: u64 = 49;

#[test]
fn stage_c_succeeds_on_the_save_after_the_hop() {
    let mut episode = start(Stage::C, arcade(), STAGE_C_LUCKY_SEED);

    let status = run_to_end(&mut episode);

    assert_eq!(status, EpisodeStatus::Succeeded);
    assert!(episode.saves() >= 1);
    assert!(!episode.obstacle_hit());
    assert!(episode.driver().is_some());
}

/// Runs `stage` from `seed` in Arcade with the landing heading limit set to `limit_deg`.
fn run_with_heading_limit(stage: Stage, seed: u64, limit_deg: f32) -> (EpisodeStatus, Episode) {
    let mut config = scenarios();
    config.episode.max_landing_heading_error_deg = limit_deg;
    let variation = EpisodeVariation::none(&vehicle(), arcade());
    let mut episode = Episode::start(&variation, &config, stage, seed).unwrap();
    let status = run_to_end(&mut episode);
    (status, episode)
}

#[test]
fn a_stage_c_landing_that_turns_past_the_limit_fails_the_episode_at_once() {
    // A tiny limit: the hop's landing always turns the nose a little.
    let (status, episode) = run_with_heading_limit(Stage::C, STAGE_C_LUCKY_SEED, 0.001);

    let error_deg = episode.result().landing_heading_error_deg.unwrap();
    assert_eq!(status, EpisodeStatus::BadLanding);
    assert!(error_deg >= 0.001, "{error_deg}");
    assert!(!episode.result().success);
}

#[test]
fn a_stage_c_landing_within_the_limit_still_succeeds() {
    let (status, episode) = run_with_heading_limit(Stage::C, STAGE_C_LUCKY_SEED, 30.0);

    let error_deg = episode.result().landing_heading_error_deg.unwrap();
    assert_eq!(status, EpisodeStatus::Succeeded);
    assert!(error_deg < 30.0, "{error_deg}");
}

#[test]
fn a_stage_d_jump_landing_past_the_limit_fails_the_episode() {
    let mut bad_landings = 0;
    for seed in 0..5 {
        let (status, episode) = run_with_heading_limit(Stage::D, seed, 0.001);
        if status == EpisodeStatus::BadLanding {
            bad_landings += 1;
            assert!(
                !episode.world().is_finished(),
                "seed {seed} ended at the landing"
            );
        }
    }
    assert!(bad_landings > 0, "no Stage D jump landing was judged");
}

#[test]
fn the_heading_limit_does_not_apply_to_stage_a_drops() {
    let mut config = scenarios();
    config.episode.max_landing_heading_error_deg = 0.001;
    let variation = EpisodeVariation::none(&vehicle(), arcade());
    for seed in 0..10 {
        let mut episode = Episode::start(&variation, &config, Stage::A, seed).unwrap();

        assert_ne!(
            run_to_end(&mut episode),
            EpisodeStatus::BadLanding,
            "seed {seed}"
        );
    }
}

#[test]
fn stage_c_fails_when_the_car_hits_the_row() {
    // Five 1.5 m cars: no hop clears them, so the car lands on (or in) the row.
    let row = PieceSpec::CarRow {
        pos: [40.0, 0.0, 0.0],
        yaw: 0.0,
        count: 5,
        height: 1.5,
        hop_hint: true,
    };
    let mut episode = start_on(&straight(vec![row]), Stage::C, 1);

    let status = run_to_end(&mut episode);

    // Touching the row ends the episode at once (or the impact crashes the car).
    let hit = status == EpisodeStatus::HitObstacle && episode.obstacle_hit();
    assert!(
        hit || matches!(status, EpisodeStatus::Crashed(_)),
        "{status:?}"
    );
    assert!(episode.result().duration_s < 10.0);
}

#[test]
fn stage_d_succeeds_only_at_the_finish_line() {
    let finish = PieceSpec::Finish {
        pos: [60.0, 0.0, 0.0],
        yaw: 0.0,
    };
    let mut episode = start_on(&straight(vec![finish]), Stage::D, 0);

    let status = run_to_end(&mut episode);

    assert_eq!(status, EpisodeStatus::Succeeded);
    assert!(episode.world().is_finished());
    assert!(episode.world().chassis_pose().position_m[0] >= 60.0);
}

#[test]
fn stage_d_without_a_finish_runs_to_its_own_time_limit() {
    let mut episode = start_on(&straight(Vec::new()), Stage::D, 0);

    let status = run_to_end(&mut episode);

    // The car may also leave the 400 m course at its far end; either way no success.
    assert_ne!(status, EpisodeStatus::Succeeded);
    if status == EpisodeStatus::TimedOut {
        let limit_s = scenarios().stage_d.time_limit_s;
        assert!((episode.result().duration_s - limit_s).abs() < 0.01);
    }
}

#[test]
fn stage_c_has_its_own_time_limit() {
    let mut config = scenarios();
    config.stage_c.time_limit_s = 0.5;
    let variation = EpisodeVariation::none(&vehicle(), realistic());
    let mut episode = Episode::start(&variation, &config, Stage::C, 0).unwrap();

    let status = run_to_end(&mut episode);

    assert_eq!(status, EpisodeStatus::TimedOut);
    assert!((episode.result().duration_s - 0.5).abs() < 0.01);
}

#[test]
fn stage_e_succeeds_only_after_its_last_knock_and_the_hold_after_it() {
    // A gentle drop with knocks of zero strength: without the knocks it would succeed
    // about a second in, so only the rule can hold it back.
    let mut config = scenarios();
    config.stage_a.height_m = [1.0, 1.0];
    config.stage_a.max_tilt_deg = 0.0;
    config.stage_a.max_spin_rad_s = 0.0;
    config.stage_a.horizontal_speed_m_s = [0.0, 0.0];
    config.stage_a.vertical_speed_m_s = [0.0, 0.0];
    config.stage_e.base_stage_weights = [1.0, 0.0, 0.0];
    config.stage_e.knock_time_s = [3.0, 4.0];
    let mut car = vehicle();
    car.chaos.max_linear_impulse_n_s = 0.0;
    car.chaos.max_angular_impulse_n_m_s = 0.0;
    let variation = EpisodeVariation::none(&car, realistic());
    for seed in 0..5 {
        let plan = draw_chaos_plan(&config.stage_e, seed);
        let last_knock_s = *plan.knock_steps.last().unwrap() as f32 / PHYSICS_HZ as f32;
        let mut episode = Episode::start(&variation, &config, Stage::E, seed).unwrap();

        let status = run_to_end(&mut episode);

        let ended_s = episode.result().duration_s;
        let earliest_s = last_knock_s + config.episode.success_hold_s;
        assert_eq!(status, EpisodeStatus::Succeeded, "seed {seed}");
        assert!(ended_s >= earliest_s - 0.01, "seed {seed}: {ended_s} s");
        assert!(ended_s < earliest_s + 0.1, "seed {seed}: {ended_s} s");
    }
}

#[test]
fn stage_c_rows_appear_in_about_half_the_episodes_and_only_when_clearable() {
    let config = scenarios();
    let clearance_m = config.driver_bot.hop_clearance_m;
    let mut with_row = 0;
    for seed in 0..400 {
        let profile = HopProfile::new(&vehicle(), &arcade());
        let setup = draw_hop_setup(&config.stage_c, clearance_m, &vehicle(), &profile, seed);
        if let Some(row) = setup.car_row {
            with_row += 1;
            assert!((1..=3).contains(&row.count));
            assert!(row.start_m > 0.0, "the row starts ahead of the car");
        }
    }
    // Arcade hops clear every row at most speeds; Realistic clears far fewer.
    assert!((120..=220).contains(&with_row), "{with_row} rows in 400");
}

#[test]
fn a_hop_whose_wheel_rays_brush_the_roof_but_clear_it_is_not_a_hit() {
    // Seed 5 drives at 23 m/s; the hop clears this 1.3 m car with so little room that
    // the suspension rays (which reach below the wheels) touch its roof.
    let row = PieceSpec::CarRow {
        pos: [60.0, 0.0, 0.0],
        yaw: 0.0,
        count: 1,
        height: 1.3,
        hop_hint: true,
    };
    let mut episode = start_on(&straight(vec![row]), Stage::C, 5);
    let mut rays_touched_the_roof = false;
    let mut status = EpisodeStatus::Running;

    while status == EpisodeStatus::Running {
        status = episode.step(NO_THRUST);
        let x_m = episode.world().chassis_pose().position_m[0];
        let over_the_car = (58.1..=66.4).contains(&x_m);
        rays_touched_the_roof |= over_the_car && episode.world().wheels_touching_count() > 0;
    }

    assert!(
        rays_touched_the_roof,
        "the test case no longer clears narrowly"
    );
    assert_eq!(status, EpisodeStatus::Succeeded);
    assert!(!episode.obstacle_hit());
}

#[test]
fn stage_e_on_c_gets_a_track_long_enough_for_es_time_limit() {
    // No row and a Hop only at 40 s: the car drives straight for 40 s. Sized by Stage
    // C's 2 s limit, the track would end long before and the car would leave it.
    let mut config = scenarios();
    config.stage_c.time_limit_s = 2.0;
    config.stage_c.car_row_probability = 0.0;
    config.stage_c.hop_time_s = [40.0, 40.0];
    config.stage_e.base_stage_weights = [0.0, 1.0, 0.0];
    config.stage_e.knock_time_s = [0.5, 1.0];
    let mut car = vehicle();
    car.chaos.max_linear_impulse_n_s = 0.0;
    car.chaos.max_angular_impulse_n_m_s = 0.0;
    let variation = EpisodeVariation::none(&car, realistic());
    let mut episode = Episode::start(&variation, &config, Stage::E, 2).unwrap();

    let status = run_to_end(&mut episode);

    assert!(!matches!(status, EpisodeStatus::Crashed(_)), "{status:?}");
    assert!(episode.result().duration_s > 40.0);
}

#[test]
fn stage_a_can_start_on_a_course() {
    let course = CourseSpec::from_json_str(GAUNTLET_JSON).unwrap();
    let mut episode = start_on(&course, Stage::A, 5);

    let status = run_to_end(&mut episode);

    assert_ne!(status, EpisodeStatus::Running);
    assert!(episode.driver().is_none());
}
