//! The training viewer's landing rates (plan G1, SPEC §11.6): flies every shipped viewer
//! checkpoint through the same `Crowd` the browser runs (the Hybrid with that checkpoint
//! as its AI), on each stage the viewer covers, and counts how the cars end. Native, with
//! fixed seeds, so the numbers can be reproduced exactly:
//!
//!     cargo run --release -p nl-wasm --example viewer_rates -- \
//!         --checkpoints web/public/checkpoints \
//!         --out results/baseline_2026-10/viewer_landing_rates.json
//!
//! Options: `--mode realistic|arcade` (default realistic, what the viewer flies),
//! `--cars 64` (cars per stage and checkpoint), `--crowd 8` (cars per crowd, what the
//! viewer flies: on D and F a crowd shares one generated course) and `--seed 0` (the first
//! drop seed; crowd `k` starts at seed `crowd * k`). The file records
//! `physics_hash`, `controller_config_hash`, each checkpoint's `policy_hash` and
//! `git_sha` (`nl_sim::provenance`, as the eval artifacts do).

use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

use nl_control::config::DEFAULT_CONTROLLERS_TOML;
use nl_sim::provenance::{default_physics_hash, text_hash};
use nl_wasm::crowd::{Crowd, CrowdOutcome, CrowdStage};
use serde_json::{Value, json};

/// How many physics steps a crowd may run (the longest stage limit is far below this).
const STEP_LIMIT: usize = 120 * 600;

/// The command line.
struct Args {
    checkpoints: PathBuf,
    out: PathBuf,
    mode: String,
    cars: u32,
    crowd: u32,
    seed: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    let index_text = std::fs::read_to_string(args.checkpoints.join("index.json"))?;
    let index: Value = serde_json::from_str(&index_text)?;
    let run = index["runs"]
        .as_array()
        .and_then(|runs| runs.last())
        .ok_or("index.json lists no runs")?;
    let run_id = run["run_id"].as_str().ok_or("run without a run_id")?;
    let entries = run["checkpoints"]
        .as_array()
        .ok_or("run without checkpoints")?;
    let mut rows = Vec::new();
    for entry in entries {
        let file = entry["file"].as_str().ok_or("checkpoint without a file")?;
        let policy_json = std::fs::read_to_string(args.checkpoints.join(run_id).join(file))?;
        let mut stages = serde_json::Map::new();
        for name in CrowdStage::NAMES {
            let counts = fly(&policy_json, &args, name)?;
            eprintln!("{run_id}/{file} {name}: {counts}");
            stages.insert(name.to_string(), counts);
        }
        rows.push(json!({
            "file": file,
            "steps": entry["steps"],
            "policy_hash": text_hash(&[&policy_json]),
            "stages": stages,
        }));
    }
    let report = json!({
        "what": "Viewer landing rates: the Hybrid (controllers.toml hand-over) flown by the viewer's Crowd, one crowd per checkpoint and stage",
        "run_id": run_id,
        "mode": args.mode,
        "cars": args.cars,
        "cars_per_crowd": args.crowd,
        "first_seed": args.seed,
        "seeds": format!("{}..{}", args.seed, args.seed + u64::from(args.cars)),
        "stages": CrowdStage::NAMES,
        "outcome_meaning": {
            "landed": "the episode succeeded; on the drops (A, arena) also a time-out after a save without a crash, as the viewer counts it",
            "crashed": "crashed, hit a parked car or turned over on a landing",
            "still_flying": "the stage's time limit ran out with no landing counted",
        },
        "physics_hash": default_physics_hash(),
        "controller_config_hash": text_hash(&[DEFAULT_CONTROLLERS_TOML]),
        "git_sha": git_sha(),
        "checkpoints": rows,
    });
    std::fs::write(&args.out, serde_json::to_string_pretty(&report)? + "\n")?;
    eprintln!("wrote {}", args.out.display());
    Ok(())
}

/// Flies `args.cars` cars of `stage` in crowds of `args.crowd`, as the viewer does (each
/// crowd is one round of drops, and on D and F one generated course), and counts how they end.
fn fly(policy_json: &str, args: &Args, stage: &str) -> Result<Value, Box<dyn Error>> {
    let (mut landed, mut crashed, mut flying, mut cars) = (0, 0, 0, 0);
    let mut first_seed = args.seed;
    while cars < args.cars {
        let size = args.crowd.min(args.cars - cars);
        let mut crowd = Crowd::for_stage(policy_json, &args.mode, stage, first_seed, size)
            .map_err(|_| format!("could not build a {stage} crowd"))?;
        let mut steps = 0;
        while crowd.step() {
            steps += 1;
            if steps > STEP_LIMIT {
                return Err(format!("the {stage} crowd never finished").into());
            }
        }
        for outcome in crowd.outcomes() {
            match outcome {
                o if o == CrowdOutcome::Landed as u8 => landed += 1,
                o if o == CrowdOutcome::Crashed as u8 => crashed += 1,
                _ => flying += 1,
            }
        }
        cars += size;
        first_seed += u64::from(size);
    }
    Ok(json!({
        "cars": cars,
        "landed": landed,
        "crashed": crashed,
        "still_flying": flying,
        "landed_rate": f64::from(landed) / f64::from(cars.max(1)),
    }))
}

/// The commit the run was made from, with "-dirty" if configs or code differ from it
/// (the same rule as the eval example); "unknown" outside a checkout.
fn git_sha() -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    let Some(sha) = git(&["rev-parse", "HEAD"]).filter(|sha| !sha.is_empty()) else {
        return "unknown".to_string();
    };
    match git(&["status", "--porcelain", "--", "configs", "crates", "python"]) {
        Some(changed) if !changed.is_empty() => format!("{sha}-dirty"),
        _ => sha,
    }
}

/// Reads `--name value` pairs.
fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut args = Args {
        checkpoints: PathBuf::from("web/public/checkpoints"),
        out: PathBuf::from("results/baseline_2026-10/viewer_landing_rates.json"),
        mode: "realistic".to_string(),
        cars: 64,
        crowd: 8,
        seed: 0,
    };
    let mut items = std::env::args().skip(1);
    while let Some(flag) = items.next() {
        let value = items
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--checkpoints" => args.checkpoints = PathBuf::from(value),
            "--out" => args.out = PathBuf::from(value),
            "--mode" => args.mode = value,
            "--cars" => args.cars = value.parse()?,
            "--crowd" => args.crowd = value.parse()?,
            "--seed" => args.seed = value.parse()?,
            other => return Err(format!("unknown option {other}").into()),
        }
    }
    Ok(args)
}
