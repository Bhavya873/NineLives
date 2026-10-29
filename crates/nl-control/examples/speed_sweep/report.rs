//! Turns the speed sweep's runs into its result files: a summary per piece, mode and
//! speed (JSON and CSV), the speed where each piece starts to crash, one example run of
//! each crashing cell, and optionally every run as CSV.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;

use serde::Serialize;

use super::args::Args;
use super::{PieceCase, RunRecord};

/// The numbers for one piece, mode and speed, over its seeds.
#[derive(Debug, Serialize)]
struct Cell {
    piece: String,
    mode: &'static str,
    speed_m_s: f32,
    runs: u32,
    survived: u32,
    crashed: u32,
    timeout: u32,
    cleared: u32,
    /// Crashes by failure class (see `classify` in main.rs).
    classes: BTreeMap<&'static str, u32>,
    peak_compression_max: f32,
    /// Runs where a wheel bottomed out at least once.
    bottomed_runs: u32,
    peak_load_g_max: f32,
    landings: u32,
    landing_fall_m_s_mean: f32,
    landing_fall_m_s_max: f32,
    landing_distance_m_mean: f32,
    landing_slope_deg_mean: f32,
    air_used_kg_mean: f32,
    air_used_kg_max: f32,
    /// Fastest body contact among runs that survived (scrape severity), in m/s.
    survived_contact_m_s_max: f32,
    /// Survived runs with any underbody contact (plan §1.3 geometry).
    survived_underbody_runs: u32,
    /// Normal speed of the crash contacts, in m/s.
    crash_contact_m_s_min: f32,
    crash_contact_m_s_max: f32,
    /// Crash contacts that pass the plan §1.3 underbody geometry test.
    crash_underbody: u32,
}

/// Where a piece starts to crash, in one mode.
#[derive(Debug, Serialize)]
struct Onset {
    piece: String,
    mode: &'static str,
    /// The lowest entry speed with any crash, and with crashes in at least half the seeds.
    first_crash_m_s: Option<f32>,
    first_majority_crash_m_s: Option<f32>,
    /// Every speed with a crash.
    crash_speeds_m_s: Vec<f32>,
}

/// Writes `<out>.json` and `<out>.csv`.
pub fn write_summary(
    out: &str,
    records: &[RunRecord],
    cases: &[PieceCase],
    args: &Args,
) -> Result<(), Box<dyn Error>> {
    let cells = make_cells(records);
    let onsets = make_onsets(&cells, cases);
    let examples = crash_examples(records);
    let report = serde_json::json!({
        "about": "Speed sweep (plan 2026-10-27 R2c): clean aligned entry, PD, per piece/mode/speed over seeds. See docs/research/speed-survival.md.",
        "seeds": args.seeds,
        "speeds_m_s": args.speeds,
        "vehicle": args.vehicle,
        "onsets": onsets,
        "cells": cells,
        "crash_examples": examples,
    });
    if let Some(folder) = std::path::Path::new(out).parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(format!("{out}.json"), serde_json::to_string(&report)?)?;
    std::fs::write(format!("{out}.csv"), cells_csv(&cells))?;
    print_onsets(&onsets);
    eprintln!("wrote {out}.json and {out}.csv");
    Ok(())
}

/// Groups the runs by piece, mode and speed, in run order.
fn make_cells(records: &[RunRecord]) -> Vec<Cell> {
    let mut cells: Vec<Cell> = Vec::new();
    for record in records {
        let same = cells.last().is_some_and(|cell| {
            cell.piece == record.piece
                && cell.mode == record.mode
                && cell.speed_m_s == record.speed_m_s
        });
        if !same {
            cells.push(Cell::new(record));
        }
        if let Some(cell) = cells.last_mut() {
            cell.add(record);
        }
    }
    for cell in &mut cells {
        cell.finish();
    }
    cells
}

impl Cell {
    fn new(record: &RunRecord) -> Cell {
        Cell {
            piece: record.piece.clone(),
            mode: record.mode,
            speed_m_s: record.speed_m_s,
            runs: 0,
            survived: 0,
            crashed: 0,
            timeout: 0,
            cleared: 0,
            classes: BTreeMap::new(),
            peak_compression_max: 0.0,
            bottomed_runs: 0,
            peak_load_g_max: 0.0,
            landings: 0,
            landing_fall_m_s_mean: 0.0,
            landing_fall_m_s_max: 0.0,
            landing_distance_m_mean: 0.0,
            landing_slope_deg_mean: 0.0,
            air_used_kg_mean: 0.0,
            air_used_kg_max: 0.0,
            survived_contact_m_s_max: 0.0,
            survived_underbody_runs: 0,
            crash_contact_m_s_min: f32::INFINITY,
            crash_contact_m_s_max: 0.0,
            crash_underbody: 0,
        }
    }

    /// Adds one run (means are sums until `finish`).
    fn add(&mut self, record: &RunRecord) {
        self.runs += 1;
        match record.outcome {
            "survived" => self.survived += 1,
            "crashed" => self.crashed += 1,
            _ => self.timeout += 1,
        }
        self.cleared += u32::from(record.cleared);
        if let Some(class) = record.failure_class {
            *self.classes.entry(class).or_insert(0) += 1;
        }
        self.peak_compression_max = self.peak_compression_max.max(record.peak_compression);
        self.bottomed_runs += u32::from(record.bottoming_events > 0);
        self.peak_load_g_max = self.peak_load_g_max.max(record.peak_load_g);
        if let Some(landing) = &record.landing {
            self.landings += 1;
            self.landing_fall_m_s_mean += landing.fall_m_s;
            self.landing_fall_m_s_max = self.landing_fall_m_s_max.max(landing.fall_m_s);
            self.landing_distance_m_mean += landing.distance_m;
            self.landing_slope_deg_mean += landing.surface_slope_deg;
        }
        self.air_used_kg_mean += record.air_used_kg;
        self.air_used_kg_max = self.air_used_kg_max.max(record.air_used_kg);
        self.add_contacts(record);
    }

    fn add_contacts(&mut self, record: &RunRecord) {
        if record.outcome == "survived"
            && let Some(contact) = &record.worst_contact
        {
            self.survived_contact_m_s_max =
                self.survived_contact_m_s_max.max(contact.normal_speed_m_s);
            self.survived_underbody_runs += u32::from(contact.underbody);
        }
        if let Some(contact) = &record.crash_contact {
            self.crash_contact_m_s_min = self.crash_contact_m_s_min.min(contact.normal_speed_m_s);
            self.crash_contact_m_s_max = self.crash_contact_m_s_max.max(contact.normal_speed_m_s);
            self.crash_underbody += u32::from(contact.underbody);
        }
    }

    /// Turns sums into means.
    fn finish(&mut self) {
        if self.landings > 0 {
            let landings = self.landings as f32;
            self.landing_fall_m_s_mean /= landings;
            self.landing_distance_m_mean /= landings;
            self.landing_slope_deg_mean /= landings;
        }
        self.air_used_kg_mean /= self.runs.max(1) as f32;
        if !self.crash_contact_m_s_min.is_finite() {
            self.crash_contact_m_s_min = 0.0;
        }
    }
}

/// The crash onset of every piece in each mode.
fn make_onsets(cells: &[Cell], cases: &[PieceCase]) -> Vec<Onset> {
    let mut onsets = Vec::new();
    for case in cases {
        for mode in ["realistic", "arcade"] {
            let mut onset = Onset {
                piece: case.name.clone(),
                mode,
                first_crash_m_s: None,
                first_majority_crash_m_s: None,
                crash_speeds_m_s: Vec::new(),
            };
            for cell in cells {
                if cell.piece != case.name || cell.mode != mode || cell.crashed == 0 {
                    continue;
                }
                onset.crash_speeds_m_s.push(cell.speed_m_s);
                onset.first_crash_m_s.get_or_insert(cell.speed_m_s);
                if 2 * cell.crashed >= cell.runs {
                    onset.first_majority_crash_m_s.get_or_insert(cell.speed_m_s);
                }
            }
            onsets.push(onset);
        }
    }
    onsets
}

/// The lowest-seed crashed run of every cell that crashed.
fn crash_examples(records: &[RunRecord]) -> Vec<&RunRecord> {
    let mut examples: Vec<&RunRecord> = Vec::new();
    for record in records.iter().filter(|record| record.outcome == "crashed") {
        let seen = examples.iter().any(|example| {
            example.piece == record.piece
                && example.mode == record.mode
                && example.speed_m_s == record.speed_m_s
        });
        if !seen {
            examples.push(record);
        }
    }
    examples
}

fn cells_csv(cells: &[Cell]) -> String {
    let mut text = String::from(
        "piece,mode,speed_m_s,runs,survived,crashed,timeout,cleared,classes,peak_compression_max,bottomed_runs,peak_load_g_max,landings,landing_fall_m_s_mean,landing_fall_m_s_max,landing_distance_m_mean,landing_slope_deg_mean,air_used_kg_mean,air_used_kg_max,survived_contact_m_s_max,survived_underbody_runs,crash_contact_m_s_min,crash_contact_m_s_max,crash_underbody\n",
    );
    for cell in cells {
        let classes: Vec<String> = cell
            .classes
            .iter()
            .map(|(class, count)| format!("{class}:{count}"))
            .collect();
        let _ = writeln!(
            text,
            "{},{},{},{},{},{},{},{},{},{:.3},{},{:.2},{},{:.2},{:.2},{:.1},{:.1},{:.2},{:.2},{:.2},{},{:.2},{:.2},{}",
            cell.piece,
            cell.mode,
            cell.speed_m_s,
            cell.runs,
            cell.survived,
            cell.crashed,
            cell.timeout,
            cell.cleared,
            classes.join(";"),
            cell.peak_compression_max,
            cell.bottomed_runs,
            cell.peak_load_g_max,
            cell.landings,
            cell.landing_fall_m_s_mean,
            cell.landing_fall_m_s_max,
            cell.landing_distance_m_mean,
            cell.landing_slope_deg_mean,
            cell.air_used_kg_mean,
            cell.air_used_kg_max,
            cell.survived_contact_m_s_max,
            cell.survived_underbody_runs,
            cell.crash_contact_m_s_min,
            cell.crash_contact_m_s_max,
            cell.crash_underbody
        );
    }
    text
}

fn print_onsets(onsets: &[Onset]) {
    println!("piece               mode       first_crash  first_majority  crash_speeds");
    for onset in onsets {
        println!(
            "{:<19} {:<10} {:>11} {:>15}  {:?}",
            onset.piece,
            onset.mode,
            speed_text(onset.first_crash_m_s),
            speed_text(onset.first_majority_crash_m_s),
            onset.crash_speeds_m_s
        );
    }
}

fn speed_text(speed_m_s: Option<f32>) -> String {
    match speed_m_s {
        Some(speed_m_s) => format!("{speed_m_s}"),
        None => "-".to_string(),
    }
}

/// One CSV row per run: the outcome, class, and the crash contact's key numbers.
pub fn write_runs_csv(path: &str, records: &[RunRecord]) -> Result<(), Box<dyn Error>> {
    let mut text = String::from(
        "piece,mode,speed_m_s,seed,outcome,crash_cause,failure_class,cleared,end_x_m,time_s,peak_compression,bottoming_events,peak_load_g,load_at_first_body_contact_g,airtime_s,distance_m,fall_m_s,land_speed_m_s,slope_deg,land_x_m,air_used_kg,peak_tilt_deg,saves,hard_landings,worst_hit,worst_normal_m_s,worst_underbody,crash_part,crash_hit,crash_px,crash_py,crash_pz,crash_nx,crash_ny,crash_nz,crash_world_ny,crash_normal_m_s,crash_underbody,crash_tilt_deg,crash_time_s\n",
    );
    for record in records {
        let _ = writeln!(text, "{}", run_row(record));
    }
    std::fs::write(path, text)?;
    Ok(())
}

fn run_row(record: &RunRecord) -> String {
    let mut row = format!(
        "{},{},{},{},{},{},{},{},{:.1},{:.2},{:.3},{},{:.2},{}",
        record.piece,
        record.mode,
        record.speed_m_s,
        record.seed,
        record.outcome,
        record.crash_cause.as_deref().unwrap_or(""),
        record.failure_class.unwrap_or(""),
        record.cleared,
        record.end_x_m,
        record.time_s,
        record.peak_compression,
        record.bottoming_events,
        record.peak_load_g,
        record
            .load_at_first_body_contact_g
            .map_or(String::new(), |g| format!("{g:.2}")),
    );
    match &record.landing {
        Some(l) => {
            let _ = write!(
                row,
                ",{:.2},{:.1},{:.2},{:.2},{:.1},{:.1}",
                l.airtime_s, l.distance_m, l.fall_m_s, l.speed_m_s, l.surface_slope_deg, l.land_x_m
            );
        }
        None => row.push_str(",,,,,,"),
    }
    let _ = write!(
        row,
        ",{:.2},{:.1},{},{}",
        record.air_used_kg, record.peak_tilt_deg, record.saves, record.hard_landings
    );
    match &record.worst_contact {
        Some(c) => {
            let _ = write!(row, ",{},{:.2},{}", c.hit, c.normal_speed_m_s, c.underbody);
        }
        None => row.push_str(",,,"),
    }
    match &record.crash_contact {
        Some(c) => {
            let [px, py, pz] = c.point_body_m;
            let [nx, ny, nz] = c.normal_body;
            let _ = write!(
                row,
                ",{:?},{},{px:.2},{py:.2},{pz:.2},{nx:.2},{ny:.2},{nz:.2},{:.4},{:.2},{},{:.1},{:.2}",
                c.part,
                c.hit,
                c.normal_world[1],
                c.normal_speed_m_s,
                c.underbody,
                c.tilt_deg,
                c.time_s
            );
        }
        None => row.push_str(",,,,,,,,,,,,,"),
    }
    row
}
