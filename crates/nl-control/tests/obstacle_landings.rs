//! PD lands an upright car dropped onto car rows, table tops, a landing pad and the ground
//! by a wall, in limited air (Realistic): at least 90% of the drops end on four wheels
//! without a crash. cargo test --release -p nl-control --test obstacle_landings -- --nocapture
//! prints the table (the example `obstacle_landings` prints it too).

mod obstacle_cases;

/// The ship gate (plan 2026-11-05 stream 3): this share of drops must land clean.
const MIN_CLEAN_SHARE: f32 = 0.9;

#[test]
fn drops_onto_obstacles_land_clean_in_limited_air() {
    let rows = obstacle_cases::all_drops();
    let (clean, total) = obstacle_cases::print_report(&rows);
    let share = clean as f32 / total as f32;
    assert!(
        share >= MIN_CLEAN_SHARE,
        "{clean} of {total} drops landed clean ({share:.2})"
    );
}
