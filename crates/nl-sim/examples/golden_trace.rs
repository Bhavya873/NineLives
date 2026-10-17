//! Runs the golden trace and prints its result as JSON.
//!
//! Regenerate the stored expectation after an intended physics change with:
//! `cargo run -p nl-sim --example golden_trace > crates/nl-sim/tests/golden/flat_drive.json`
//! and explain the change in the PR. Never update it just to make a test pass.

use std::error::Error;

use nl_sim::golden_trace::run_golden_trace;

fn main() -> Result<(), Box<dyn Error>> {
    let result = run_golden_trace()?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
