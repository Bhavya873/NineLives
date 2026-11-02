//! Times the viewer's crowd: 8 checkpoints x 8 cars, as `web/src/viewer/main.ts` steps it.
//! Prints ms per tick (all 64 cars) and per frame read (poses + outcomes). Run with
//! `cargo run --release -p nl-wasm --example crowd_bench [stage]`.
use std::time::Instant;

use nl_wasm::crowd::Crowd;

const POLICY_JSON: &str = include_str!("../../../models/policy.json");

fn main() {
    let stage = std::env::args().nth(1).unwrap_or_else(|| "A".to_string());
    let mut rows: Vec<Crowd> = (0..8)
        .map(|_| {
            let Ok(crowd) = Crowd::for_stage(POLICY_JSON, "realistic", &stage, 1000, 8) else {
                panic!("could not build a crowd for stage {stage}");
            };
            crowd
        })
        .collect();
    let mut ticks = 0u32;
    let mut step_s = 0.0;
    let mut read_s = 0.0;
    let mut checksum = 0.0f64;
    while ticks < 1200 {
        let start = Instant::now();
        let mut any = false;
        for row in &mut rows {
            any |= row.step();
        }
        step_s += start.elapsed().as_secs_f64();
        ticks += 1;
        if ticks % 2 == 0 {
            let start = Instant::now();
            for row in &rows {
                checksum += f64::from(row.chassis_poses().iter().sum::<f32>());
                checksum += f64::from(row.wheel_poses().iter().sum::<f32>());
                checksum += row.outcomes().iter().map(|&c| f64::from(c)).sum::<f64>();
            }
            read_s += start.elapsed().as_secs_f64();
        }
        if !any {
            break;
        }
    }
    println!(
        "stage {stage}: {ticks} ticks, {:.3} ms/tick step, {:.3} ms per frame read, checksum {checksum:.4}",
        step_s * 1000.0 / f64::from(ticks),
        read_s * 1000.0 / f64::from(ticks / 2),
    );
}
