//! The Rust eval runner's command line (SPEC §10). Runs Off, PD or the AI policy through
//! seeded episodes of Stage A or B and writes `results/eval_<controller>_<stage>_<mode>.json`.
//!
//! cargo run --release -p nl-control --example eval -- \
//!     --controller pd --stage A --mode realistic --episodes 1000 --seed 0
//! (`--controller ai` reads `--policy`, default models/policy.json; `--dr` turns on
//! domain randomization, SPEC §9.8, and adds `_dr` to the file name)

use std::error::Error;

use nl_control::ControllerKind;
use nl_control::build_controller;
use nl_control::config::DEFAULT_CONTROLLERS_TOML;
use nl_control::eval::{run_episode, summarise};
use nl_sim::episode::EpisodeResult;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModeParams, ModesConfig};
use nl_sim::randomization::EpisodeVariation;
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig, Stage};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};

/// What the command line asked for.
struct Args {
    controller: String,
    stage: String,
    mode: String,
    episodes: u64,
    seed: u64,
    policy: String,
    randomize: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML)?;
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)?;
    let scenarios = ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML)?;
    let stage = Stage::from_name(&args.stage).ok_or("--stage must be A or B")?;
    let mode = modes
        .by_name(&args.mode)
        .ok_or("--mode must be realistic or arcade")?;

    let mut controller = build_named_controller(&args, &vehicle, mode)?;
    let randomization = args.randomize.then_some(&scenarios.randomization);
    let mut results: Vec<EpisodeResult> = Vec::new();
    for episode in 0..args.episodes {
        let seed = args
            .seed
            .checked_add(episode)
            .ok_or("--seed is too large")?;
        let variation = EpisodeVariation::for_seed(randomization, &vehicle, mode, seed);
        let result = run_episode(&variation, &scenarios, stage, &mut controller, seed)?;
        results.push(result);
    }
    write_report(&args, &results)
}

/// Prints the summary and writes it, with every episode, to the results file.
fn write_report(args: &Args, results: &[EpisodeResult]) -> Result<(), Box<dyn Error>> {
    let summary = summarise(results);
    let stage_name = args.stage.to_uppercase();
    let report = serde_json::json!({
        "controller": args.controller,
        "stage": stage_name,
        "mode": args.mode,
        "first_seed": args.seed,
        "randomized": args.randomize,
        "summary": summary,
        "episodes": results,
    });
    std::fs::create_dir_all("results")?;
    let suffix = if args.randomize { "_dr" } else { "" };
    let path = format!(
        "results/eval_{}_{}_{}{suffix}.json",
        args.controller, stage_name, args.mode
    );
    // Compact: 1000 per-episode rows pretty-printed are several hundred KB per file.
    std::fs::write(&path, serde_json::to_string(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    println!("wrote {path}");
    Ok(())
}

/// The controller named by `--controller`: Off, PD built for this car, or the AI with
/// the policy from `--policy` (read only when the AI is asked for).
fn build_named_controller(
    args: &Args,
    vehicle: &VehicleConfig,
    mode: ModeParams,
) -> Result<ControllerKind, Box<dyn Error>> {
    let mut policy_json = None;
    if args.controller == "ai" {
        let text = std::fs::read_to_string(&args.policy)
            .map_err(|error| format!("--policy {}: {error}", args.policy))?;
        policy_json = Some(text);
    }
    let controllers_toml = DEFAULT_CONTROLLERS_TOML;
    let controller = build_controller(
        &args.controller,
        vehicle,
        mode,
        controllers_toml,
        policy_json.as_deref(),
    )?;
    Ok(controller)
}

/// Reads `--name value` pairs; anything missing gets the default shown in the header.
fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut args = Args {
        controller: "pd".to_string(),
        stage: "A".to_string(),
        mode: "realistic".to_string(),
        episodes: 1000,
        seed: 0,
        policy: "models/policy.json".to_string(),
        randomize: false,
    };
    let mut words = std::env::args().skip(1);
    while let Some(name) = words.next() {
        if name == "--dr" {
            args.randomize = true;
            continue;
        }
        let value = words.next().ok_or(format!("{name} needs a value"))?;
        let (name, value) = (&name, &value);
        match name.as_str() {
            "--controller" => args.controller = value.clone(),
            "--stage" => args.stage = value.clone(),
            "--mode" => args.mode = value.clone(),
            "--episodes" => args.episodes = whole_number(name, value)?,
            "--seed" => args.seed = whole_number(name, value)?,
            "--policy" => args.policy = value.clone(),
            other => return Err(format!("unknown option {other}").into()),
        }
    }
    Ok(args)
}

/// Reads `value` as a whole number, or says which option needs one.
fn whole_number(option: &str, value: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("{option} needs a whole number, got {value}"))
}
