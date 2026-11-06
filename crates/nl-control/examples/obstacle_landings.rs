//! Prints the obstacle-landing table and the high-jump tank rows (the same drops as
//! tests/obstacle_landings.rs). cargo run --release -p nl-control --example obstacle_landings

#[path = "../tests/obstacle_cases/mod.rs"]
mod obstacle_cases;

fn main() {
    let rows = obstacle_cases::all_drops();
    obstacle_cases::print_report(&rows);
}
