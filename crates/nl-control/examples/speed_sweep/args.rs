//! The speed sweep's command line: `--name value` pairs, each with a default.

use std::error::Error;

use super::Floor;

/// The playground's floor (`courses/playground.json` bounds ±800 m), so the sweep sees
/// the same contact behaviour as free driving.
const DEFAULT_FLOOR: Floor = Floor {
    behind_m: 800.0,
    ahead_m: 800.0,
    half_width_m: 800.0,
    tile_m: 0.0,
};

/// What the command line asked for.
pub struct Args {
    /// Seeds per piece, mode and speed.
    pub seeds: u64,
    /// Worker threads (keep at most 6 on the 8-core dev machine).
    pub threads: usize,
    /// A vehicle TOML to use instead of the repo's.
    pub vehicle: Option<String>,
    /// Piece names to run; empty runs every piece.
    pub pieces: Vec<String>,
    /// Entry speeds, in m/s.
    pub speeds: Vec<f32>,
    /// Output path without extension; `.json` and `.csv` are added.
    pub out: String,
    /// Where to write one CSV row per run, if anywhere.
    pub runs_csv: Option<String>,
    /// One run to print step by step, as `piece:mode:speed:seed`.
    pub trace: Option<String>,
    /// The floor's size (`--floor behind,ahead,half_width`, in m).
    pub floor: Floor,
}

/// Reads the arguments; anything missing gets the default in the module header.
pub fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut args = Args {
        seeds: 20,
        threads: 6,
        vehicle: None,
        pieces: Vec::new(),
        speeds: super::default_speeds_m_s(),
        out: "results/speed_sweep_before".to_string(),
        runs_csv: None,
        trace: None,
        floor: DEFAULT_FLOOR,
    };
    let words: Vec<String> = std::env::args().skip(1).collect();
    for pair in words.chunks(2) {
        let [name, value] = pair else {
            return Err(format!("{} needs a value", pair[0]).into());
        };
        match name.as_str() {
            "--seeds" => args.seeds = value.parse()?,
            "--threads" => args.threads = value.parse::<usize>()?.clamp(1, 6),
            "--vehicle" => args.vehicle = Some(value.clone()),
            "--pieces" => args.pieces = value.split(',').map(str::to_string).collect(),
            "--speeds" => args.speeds = parse_speeds(value)?,
            "--out" => args.out = value.clone(),
            "--runs-csv" => args.runs_csv = Some(value.clone()),
            "--trace" => args.trace = Some(value.clone()),
            "--floor" => args.floor = parse_floor(value)?,
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    Ok(args)
}

fn parse_floor(text: &str) -> Result<Floor, Box<dyn Error>> {
    let numbers = parse_speeds(text)?;
    let (behind_m, ahead_m, half_width_m, tile_m) = match numbers[..] {
        [behind, ahead, half_width] => (behind, ahead, half_width, 0.0),
        [behind, ahead, half_width, tile] => (behind, ahead, half_width, tile),
        _ => return Err("--floor needs behind,ahead,half_width[,tile] in m".into()),
    };
    Ok(Floor {
        behind_m,
        ahead_m,
        half_width_m,
        tile_m,
    })
}

/// Comma-separated numbers.
fn parse_speeds(text: &str) -> Result<Vec<f32>, Box<dyn Error>> {
    let mut speeds = Vec::new();
    for word in text.split(',') {
        speeds.push(word.parse()?);
    }
    Ok(speeds)
}
