//! The Rust eval runner's command line (SPEC §10, M4). Runs Off or PD through seeded
//! episodes of Stage A or B and writes `results/eval_<controller>_<stage>_<mode>.json`.
//!
//! cargo run --release -p nl-control --example eval -- \
//!     --controller pd --stage A --mode realistic --episodes 200 --seed 0

use std::error::Error;

use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};
use nl_control::eval::{EpisodeResult, run_episode, summarise};
use nl_control::{Controller, Off, Pd};
use nl_sim::modes::{DEFAULT_MODES_TOML, ModeParams, ModesConfig};
use nl_sim::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig, Stage};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// What the command line asked for.
struct Args {
    controller: String,
    stage: String,
    mode: String,
    episodes: u64,
    seed: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML)?;
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)?;
    let scenarios = ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML)?;
    let stage = Stage::from_name(&args.stage).ok_or("--stage must be A or B")?;
    let mode = match args.mode.as_str() {
        "realistic" => modes.realistic,
        "arcade" => modes.arcade,
        _ => return Err("--mode must be realistic or arcade".into()),
    };

    let mut controller = build_controller(&args.controller, &vehicle, mode)?;
    let mut results: Vec<EpisodeResult> = Vec::new();
    for episode in 0..args.episodes {
        let seed = args.seed + episode;
        results.push(run_episode(
            &vehicle,
            mode,
            &scenarios,
            stage,
            controller.as_mut(),
            seed,
        )?);
    }

    let summary = summarise(&results);
    let report = serde_json::json!({
        "controller": args.controller,
        "stage": args.stage.to_uppercase(),
        "mode": args.mode,
        "first_seed": args.seed,
        "summary": summary,
        "episodes": results,
    });
    std::fs::create_dir_all("results")?;
    let path = format!(
        "results/eval_{}_{}_{}.json",
        args.controller,
        args.stage.to_uppercase(),
        args.mode
    );
    std::fs::write(&path, serde_json::to_string_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    println!("wrote {path}");
    Ok(())
}

/// Off, or PD built for this car.
fn build_controller(
    name: &str,
    vehicle: &VehicleConfig,
    mode: ModeParams,
) -> Result<Box<dyn Controller>, Box<dyn Error>> {
    match name {
        "off" => Ok(Box::new(Off)),
        "pd" => {
            let world = World::new_on_flat_ground(vehicle, mode, DEFAULT_GROUND_FRICTION, 0);
            let config = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML)?;
            Ok(Box::new(Pd::new(&world.car_model(), &config)))
        }
        _ => Err("--controller must be off or pd".into()),
    }
}

/// Reads `--name value` pairs; anything missing gets the default shown in the header.
fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut args = Args {
        controller: "pd".to_string(),
        stage: "A".to_string(),
        mode: "realistic".to_string(),
        episodes: 200,
        seed: 0,
    };
    let words: Vec<String> = std::env::args().skip(1).collect();
    for pair in words.chunks(2) {
        let [name, value] = pair else {
            return Err(format!("{} needs a value", pair[0]).into());
        };
        match name.as_str() {
            "--controller" => args.controller = value.clone(),
            "--stage" => args.stage = value.clone(),
            "--mode" => args.mode = value.clone(),
            "--episodes" => args.episodes = value.parse()?,
            "--seed" => args.seed = value.parse()?,
            other => return Err(format!("unknown option {other}").into()),
        }
    }
    Ok(args)
}
