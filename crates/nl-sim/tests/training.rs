//! Training environment tests (SPEC §9.1, §9.3, §9.5, §9.6): actions map to throttles,
//! episodes end exactly as `Episode` says, runs repeat for the same seed, and the
//! Stage A curriculum widens tilt and spin level by level.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{realistic, reward, scenarios, vehicle};
use nl_sim::episode::{Episode, EpisodeStatus};
use nl_sim::randomization::EpisodeVariation;
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig, Stage};
use nl_sim::training::{
    StageChoice, StepOutcome, TrainingConfig, TrainingEnv, throttles_from_action,
};

/// All valves shut: the policy action −1 on every nozzle.
const VALVES_SHUT: [f32; 10] = [-1.0; 10];

fn config() -> TrainingConfig {
    TrainingConfig {
        vehicle: vehicle(),
        mode: realistic(),
        scenarios: scenarios(),
        reward: reward(),
        randomize: false,
    }
}

/// Steps `env` with `action` until the episode ends; returns every step's outcome.
fn run_to_end(env: &mut TrainingEnv, action: [f32; 10]) -> Vec<StepOutcome> {
    let mut outcomes = Vec::new();
    while let Some(outcome) = env.step(action) {
        let ended = outcome.terminated || outcome.truncated;
        outcomes.push(outcome);
        if ended {
            break;
        }
    }
    outcomes
}

#[test]
fn actions_map_to_throttles_as_the_spec_says() {
    let mut action = [0.0; 10];
    action[0] = -1.0;
    action[1] = 1.0;
    action[2] = 3.0;
    action[3] = f32::NAN;

    let throttles = throttles_from_action(action);

    assert_eq!(throttles[0], 0.0);
    assert_eq!(throttles[1], 1.0);
    assert_eq!(throttles[2], 1.0);
    assert_eq!(throttles[3], 0.0);
    assert_eq!(throttles[4], 0.5);
}

#[test]
fn stepping_before_reset_gives_nothing() {
    let mut env = TrainingEnv::new(config(), Stage::A);

    assert!(env.step(VALVES_SHUT).is_none());
}

#[test]
fn an_episode_ends_once_with_its_row_and_then_needs_a_reset() {
    let mut env = TrainingEnv::new(config(), Stage::A);
    env.reset(7).unwrap();

    let outcomes = run_to_end(&mut env, VALVES_SHUT);

    let (last, before_last) = outcomes.split_last().unwrap();
    assert!(last.terminated || last.truncated);
    assert!(last.finished.is_some());
    for outcome in before_last {
        assert!(!outcome.terminated && !outcome.truncated);
        assert!(outcome.finished.is_none());
    }
    let finished = last.finished.as_ref().unwrap();
    assert_eq!(finished.control_steps as usize, outcomes.len());
    assert!(env.step(VALVES_SHUT).is_none());
}

#[test]
fn shut_valves_give_the_same_episode_row_as_stepping_episode_directly() {
    // The training env must not change the episode rules (SPEC §9.5): with every valve
    // shut it is the same run as the eval runner's Off controller.
    let seed = 11;
    let mut env = TrainingEnv::new(config(), Stage::A);
    env.reset(seed).unwrap();
    let level_zero_scenarios = scenarios().at_level(Stage::A, 0);
    let plain = EpisodeVariation::none(&vehicle(), realistic());
    let mut episode = Episode::start(&plain, &level_zero_scenarios, Stage::A, seed).unwrap();

    let outcomes = run_to_end(&mut env, VALVES_SHUT);
    while episode.status() == EpisodeStatus::Running {
        episode.step([0.0; 10]);
    }

    let finished = outcomes.last().unwrap().finished.as_ref().unwrap();
    assert_eq!(finished.result, episode.result());
}

#[test]
fn the_same_seed_and_actions_repeat_exactly() {
    let mut first = TrainingEnv::new(config(), Stage::A);
    let mut second = TrainingEnv::new(config(), Stage::A);
    let mut action = VALVES_SHUT;
    action[0] = 0.2;
    action[5] = -0.4;

    assert_eq!(first.reset(3).unwrap(), second.reset(3).unwrap());
    assert_eq!(
        run_to_end(&mut first, action),
        run_to_end(&mut second, action)
    );
}

#[test]
fn a_crash_ends_the_episode_with_the_crash_penalty() {
    let hardest_level = scenarios().curriculum_a.levels - 1;
    let shape = reward().shape;
    let crash_reward = -reward().weights.crash * shape.crash_penalty;
    let mut env = TrainingEnv::new(config(), Stage::A);
    env.set_stage_and_level(Stage::A, hardest_level);
    for seed in 0..50 {
        env.reset(seed).unwrap();
        let outcomes = run_to_end(&mut env, VALVES_SHUT);
        let last = outcomes.last().unwrap();
        let crashed = last.finished.as_ref().unwrap().result.crash_cause.is_some();
        if crashed {
            assert!(last.terminated && !last.truncated);
            assert!((last.terms.crash - crash_reward).abs() < 1e-6);
            return;
        }
    }
    panic!("no crash in 50 hardest-level drops with shut valves");
}

#[test]
fn level_zero_is_gentle_and_the_last_level_is_the_full_stage() {
    let config = scenarios();
    let last_level = config.curriculum_a.levels - 1;

    let easiest = config.stage_a_at_level(0);
    let hardest = config.stage_a_at_level(last_level);
    let beyond = config.stage_a_at_level(last_level + 3);

    let first_tilt_deg = config.curriculum_a.first_max_tilt_deg;
    assert_eq!(easiest.max_tilt_deg, first_tilt_deg);
    assert_eq!(
        easiest.max_spin_rad_s,
        config.curriculum_a.first_max_spin_rad_s
    );
    assert_eq!(hardest, config.stage_a);
    assert_eq!(beyond, config.stage_a);
    let middle = config.stage_a_at_level(last_level / 2);
    assert!(middle.max_tilt_deg > first_tilt_deg);
    assert!(middle.max_tilt_deg < config.stage_a.max_tilt_deg);
}

#[test]
fn a_single_level_curriculum_is_always_the_full_stage() {
    let mut config = scenarios();
    config.curriculum_a.levels = 1;

    assert_eq!(config.stage_a_at_level(0), config.stage_a);
}

#[test]
fn a_new_level_applies_to_the_next_episode_not_the_running_one() {
    let mut env = TrainingEnv::new(config(), Stage::A);
    env.reset(5).unwrap();

    env.set_stage_and_level(Stage::A, 3);
    let outcomes = run_to_end(&mut env, VALVES_SHUT);

    assert_eq!(outcomes.last().unwrap().finished.as_ref().unwrap().level, 0);
}

#[test]
fn a_curriculum_that_starts_harder_than_it_ends_is_refused_by_name() {
    let text =
        DEFAULT_SCENARIOS_TOML.replace("first_max_tilt_deg = 30.0", "first_max_tilt_deg = 200.0");

    let error = ScenariosConfig::from_toml_str(&text).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("curriculum_a.first_max_tilt_deg"),
        "{error}"
    );
}

#[test]
fn a_hard_level_drop_pays_the_hard_landing_cost_and_the_air_left_bonus() {
    let mut config = config();
    config.scenarios.curriculum_a.levels = 1;
    // 3.1 m, as in episode.rs: from 3.0 m the roof's air drag keeps it just short of
    // a hard landing.
    config.scenarios.stage_a.height_m = [3.1, 3.1];
    config.scenarios.stage_a.max_tilt_deg = 0.0;
    config.scenarios.stage_a.max_spin_rad_s = 0.0;
    config.scenarios.stage_a.horizontal_speed_m_s = [0.0, 0.0];
    config.scenarios.stage_a.vertical_speed_m_s = [0.0, 0.0];
    let mut env = TrainingEnv::new(config, Stage::A);
    env.reset(0).unwrap();

    let outcomes = run_to_end(&mut env, VALVES_SHUT);

    let mut hard_landing_cost = 0.0;
    for outcome in &outcomes {
        hard_landing_cost += outcome.terms.hard_landing;
    }
    let last = outcomes.last().unwrap();
    assert!(
        (hard_landing_cost + 1.0).abs() < 1e-6,
        "{hard_landing_cost}"
    );
    assert!(last.terminated);
    // Closed valves use no air, so the whole tank is left: weight 1.0 × 1.0.
    assert!((last.terms.air_left - 1.0).abs() < 1e-6);
}

/// The stage of the episode a reset with `seed` starts, read from its finished row.
fn stage_of_episode(env: &mut TrainingEnv, seed: u64) -> Stage {
    env.reset(seed).unwrap();
    let outcomes = run_to_end(env, VALVES_SHUT);
    outcomes.last().unwrap().finished.as_ref().unwrap().stage
}

fn choice(stage: Stage, weight: f32) -> StageChoice {
    StageChoice {
        stage,
        weight,
        level: 0,
    }
}

#[test]
fn a_stage_mix_picks_each_episodes_stage_by_weight_from_its_seed() {
    let mut env = TrainingEnv::new(config(), Stage::A);
    let mix = vec![
        choice(Stage::A, 1.0),
        choice(Stage::B, 1.0),
        choice(Stage::C, 0.0),
    ];
    env.set_stage_mix(mix).unwrap();

    let stages: Vec<Stage> = (0..20)
        .map(|seed| stage_of_episode(&mut env, seed))
        .collect();
    let again: Vec<Stage> = (0..20)
        .map(|seed| stage_of_episode(&mut env, seed))
        .collect();

    assert_eq!(stages, again, "the seed decides the stage");
    assert!(stages.contains(&Stage::A) && stages.contains(&Stage::B));
    assert!(!stages.contains(&Stage::C), "a zero weight never comes up");
}

#[test]
fn an_empty_or_weightless_stage_mix_is_refused() {
    let mut env = TrainingEnv::new(config(), Stage::A);

    assert!(env.set_stage_mix(Vec::new()).is_err());
    assert!(env.set_stage_mix(vec![choice(Stage::A, 0.0)]).is_err());
}
