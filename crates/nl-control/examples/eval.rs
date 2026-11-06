//! The Rust eval runner's command line (SPEC §10). Runs Off, PD or the AI policy through
//! seeded episodes of any stage (A to F) and writes `results/eval_<controller>_<stage>_<mode>.json`.
//!
//! cargo run --release -p nl-control --example eval -- \
//!     --controller pd --stage A --mode realistic --episodes 1000 --seed 0
//! (`--controller ai` or `hybrid` reads `--policy`, default models/policy.json; `--dr` turns on
//! domain randomization, SPEC §9.8, and adds `_dr` to the file name; `--legacy` turns the
//! scenarios.toml `[widening]` off (the Stage A and F draws every earlier result was made
//! with; the file name gets no suffix, so pair it with `--out`); `--course <json>`
//! runs the episodes on that course, e.g. courses/playground.json, and adds its file
//! stem, e.g. `_playground`; `--controllers <toml>` reads the controller settings from that
//! file instead of configs/controllers.toml, e.g. to try `[allocation] model`; `--out <dir>`
//! writes there instead of `results`)
//!
//! Every file also has `by_preset` and `by_axis` tables (episodes, landing and crash rate per
//! flip-plate preset and per dominant spin axis), printed as markdown after the summary.
//! Every file also records `physics_hash`, `controller_config_hash`, `policy_hash` (null
//! without a policy) and `git_sha`, so a result can be matched to the settings it ran on.

use std::error::Error;
use std::process::Command;

use nl_control::ControllerKind;
use nl_control::build_controller;
use nl_control::config::DEFAULT_CONTROLLERS_TOML;
use nl_control::eval::{run_episode, run_episode_on_course, summarise};
use nl_sim::course::CourseSpec;
use nl_sim::episode::EpisodeResult;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModeParams, ModesConfig};
use nl_sim::provenance::{default_physics_hash, text_hash};
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
    /// Run the Stage A and F draws as they were before `[widening]`.
    legacy: bool,
    /// A course JSON file to run the episodes on; None: flat ground.
    course: Option<String>,
    /// A controllers.toml to use instead of the repo's; None: the repo's.
    controllers: Option<String>,
    /// The folder the results file goes in.
    out: String,
}

/// What ties a results file to the settings and policy it ran on (plan G1).
struct Provenance {
    physics_hash: String,
    controller_config_hash: String,
    /// None when the controller loads no policy.
    policy_hash: Option<String>,
    git_sha: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML)?;
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)?;
    let mut scenarios = ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML)?;
    if args.legacy {
        scenarios = scenarios.legacy();
    }
    let stage = Stage::from_name(&args.stage).ok_or("--stage must be A, B, C, D, E or F")?;
    let mode = modes
        .by_name(&args.mode)
        .ok_or("--mode must be realistic or arcade")?;

    let (mut controller, provenance) = build_named_controller(&args, &vehicle, mode)?;
    let randomization = args.randomize.then_some(&scenarios.randomization);
    let course = match &args.course {
        Some(path) => Some(CourseSpec::from_json_str(&std::fs::read_to_string(path)?)?),
        None => None,
    };
    let mut results: Vec<EpisodeResult> = Vec::new();
    for episode in 0..args.episodes {
        let seed = args
            .seed
            .checked_add(episode)
            .ok_or("--seed is too large")?;
        let variation = EpisodeVariation::for_seed(randomization, &vehicle, mode, seed);
        let result = match &course {
            Some(course) => {
                run_episode_on_course(&variation, &scenarios, stage, course, &mut controller, seed)?
            }
            None => run_episode(&variation, &scenarios, stage, &mut controller, seed)?,
        };
        results.push(result);
    }
    write_report(&args, &results, &provenance, scenarios.widened().is_some())
}

/// Prints the summary and writes it, with every episode, to the results file.
fn write_report(
    args: &Args,
    results: &[EpisodeResult],
    provenance: &Provenance,
    widened: bool,
) -> Result<(), Box<dyn Error>> {
    let summary = summarise(results);
    let by_preset = group_rows(results, |row| row.preset.clone());
    let by_axis = group_rows(results, |row| {
        row.spin_axis.map(|axis| axis.name().to_string())
    });
    let stage_name = args.stage.to_uppercase();
    let report = serde_json::json!({
        "controller": args.controller,
        "stage": stage_name,
        "mode": args.mode,
        "first_seed": args.seed,
        "randomized": args.randomize,
        "widened": widened,
        "course": args.course,
        "physics_hash": provenance.physics_hash,
        "controller_config_hash": provenance.controller_config_hash,
        "policy_hash": provenance.policy_hash,
        "git_sha": provenance.git_sha,
        "summary": summary,
        "by_preset": by_preset,
        "by_axis": by_axis,
        "episodes": results,
    });
    std::fs::create_dir_all(&args.out)?;
    let mut suffix = String::new();
    if let Some(path) = &args.course {
        let stem = std::path::Path::new(path)
            .file_stem()
            .and_then(|stem| stem.to_str());
        suffix = format!("_{}", stem.unwrap_or("course"));
    }
    if args.randomize {
        suffix.push_str("_dr");
    }
    let path = format!(
        "{}/eval_{}_{}_{}{suffix}.json",
        args.out, args.controller, stage_name, args.mode
    );
    // Compact: 1000 per-episode rows pretty-printed are several hundred KB per file.
    std::fs::write(&path, serde_json::to_string(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    println!("{}", markdown_table("preset", &by_preset));
    println!("{}", markdown_table("spin axis", &by_axis));
    println!("wrote {path}");
    Ok(())
}

/// One line of the per-preset or per-axis table.
#[derive(Debug, serde::Serialize)]
struct GroupRow {
    /// The preset or axis name ("none" for episodes without one).
    name: String,
    episodes: usize,
    /// Fraction that succeeded (landed on the wheels and held), 0..1.
    landing_rate: f32,
    /// Fraction that ended in a crash, 0..1.
    crash_rate: f32,
    /// Mean strength of the group's episodes, if they have one.
    mean_strength: Option<f32>,
}

/// The episodes grouped by `key` (sorted by name), with their landing and crash rates.
fn group_rows(
    results: &[EpisodeResult],
    key: impl Fn(&EpisodeResult) -> Option<String>,
) -> Vec<GroupRow> {
    let mut groups: std::collections::BTreeMap<String, Vec<&EpisodeResult>> =
        std::collections::BTreeMap::new();
    for row in results {
        groups
            .entry(key(row).unwrap_or_else(|| "none".to_string()))
            .or_default()
            .push(row);
    }
    let rate = |rows: &[&EpisodeResult], test: fn(&EpisodeResult) -> bool| {
        rows.iter().filter(|row| test(row)).count() as f32 / rows.len() as f32
    };
    groups
        .into_iter()
        .map(|(name, rows)| {
            let strengths: Vec<f32> = rows.iter().filter_map(|row| row.strength).collect();
            let mean_strength = (!strengths.is_empty())
                .then(|| strengths.iter().sum::<f32>() / strengths.len() as f32);
            GroupRow {
                name,
                episodes: rows.len(),
                landing_rate: rate(&rows, |row| row.success),
                crash_rate: rate(&rows, |row| row.crash_cause.is_some()),
                mean_strength,
            }
        })
        .collect()
}

/// `rows` as a markdown table headed `heading`.
fn markdown_table(heading: &str, rows: &[GroupRow]) -> String {
    let mut text =
        format!("| {heading} | episodes | landing % | crash % |\n|---|---:|---:|---:|\n");
    for row in rows {
        text.push_str(&format!(
            "| {} | {} | {:.1} | {:.1} |\n",
            row.name,
            row.episodes,
            row.landing_rate * 100.0,
            row.crash_rate * 100.0
        ));
    }
    text
}

/// The controller named by `--controller`: Off, PD built for this car, or the AI or
/// Hybrid with the policy from `--policy` (read only when one of those is asked for), and
/// the fingerprints of the settings and policy it was built from.
fn build_named_controller(
    args: &Args,
    vehicle: &VehicleConfig,
    mode: ModeParams,
) -> Result<(ControllerKind, Provenance), Box<dyn Error>> {
    let mut policy_json = None;
    if args.controller == "ai" || args.controller == "hybrid" {
        let text = std::fs::read_to_string(&args.policy)
            .map_err(|error| format!("--policy {}: {error}", args.policy))?;
        policy_json = Some(text);
    }
    let controllers_toml = match &args.controllers {
        Some(path) => std::fs::read_to_string(path)
            .map_err(|error| format!("--controllers {path}: {error}"))?,
        None => DEFAULT_CONTROLLERS_TOML.to_string(),
    };
    let controller = build_controller(
        &args.controller,
        vehicle,
        mode,
        &controllers_toml,
        policy_json.as_deref(),
    )?;
    let provenance = Provenance {
        physics_hash: default_physics_hash(),
        controller_config_hash: text_hash(&[&controllers_toml]),
        policy_hash: policy_json.as_deref().map(|text| text_hash(&[text])),
        git_sha: git_sha(),
    };
    Ok((controller, provenance))
}

/// The commit the run was made from, with "-dirty" if configs or code differ from it
/// (then the commit alone cannot reproduce the run); "unknown" outside a checkout. The same
/// rule as `ninelives.export.git_sha`.
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
    let sha = git(&["rev-parse", "HEAD"]).filter(|sha| !sha.is_empty());
    let Some(sha) = sha else {
        return "unknown".to_string();
    };
    let changed = git(&["status", "--porcelain", "--", "configs", "crates", "python"]);
    match changed {
        Some(changed) if !changed.is_empty() => format!("{sha}-dirty"),
        _ => sha,
    }
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
        legacy: false,
        course: None,
        controllers: None,
        out: "results".to_string(),
    };
    let mut words = std::env::args().skip(1);
    while let Some(name) = words.next() {
        if name == "--dr" {
            args.randomize = true;
            continue;
        }
        if name == "--legacy" {
            args.legacy = true;
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
            "--course" => args.course = Some(value.clone()),
            "--controllers" => args.controllers = Some(value.clone()),
            "--out" => args.out = value.clone(),
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
