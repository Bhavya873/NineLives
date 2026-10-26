//! Procedural course tests (SPEC §7.11): every generated course passes the course-file
//! checks (written to JSON and read back unchanged), the same seed gives the same
//! course, and the layout keeps the drivability rules: 4–8 pieces, straights of 20–40 m,
//! a 33 m run-up before every jump, checkpoints on flat floor and one finish at the end.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use nl_sim::course::{CourseSpec, PieceSpec};
use nl_sim::course_gen::{DEFAULT_PIECES_JSON, PieceCatalog, generate_course};

const SEEDS: u64 = 300;

fn catalog() -> PieceCatalog {
    PieceCatalog::from_json_str(DEFAULT_PIECES_JSON).unwrap()
}

/// Pieces that are not lines or gaps: the drawn pieces (a wall's ramp is part of it).
fn drawn_pieces(course: &CourseSpec) -> Vec<&PieceSpec> {
    course
        .pieces
        .iter()
        .filter(|piece| {
            !matches!(
                piece,
                PieceSpec::Checkpoint { .. } | PieceSpec::Finish { .. } | PieceSpec::Gap { .. }
            )
        })
        .collect()
}

#[test]
fn every_generated_course_round_trips_through_the_course_file_checks() {
    let catalog = catalog();

    for seed in 0..SEEDS {
        let course = generate_course(&catalog, seed);
        let json = course.to_json().unwrap();

        let read_back = CourseSpec::from_json_str(&json)
            .unwrap_or_else(|error| panic!("seed {seed}: {error}\n{json}"));

        assert_eq!(read_back, course, "seed {seed}");
    }
}

#[test]
fn the_same_seed_gives_the_same_course_and_different_seeds_differ() {
    let catalog = catalog();

    assert_eq!(generate_course(&catalog, 7), generate_course(&catalog, 7));
    assert_ne!(generate_course(&catalog, 7), generate_course(&catalog, 8));
}

#[test]
fn courses_have_four_to_eight_pieces_a_checkpoint_after_each_but_the_last_and_one_finish() {
    let catalog = catalog();

    for seed in 0..SEEDS {
        let course = generate_course(&catalog, seed);
        let drawn = drawn_pieces(&course).len();
        let checkpoints = course
            .pieces
            .iter()
            .filter(|piece| matches!(piece, PieceSpec::Checkpoint { .. }))
            .count();
        let last = course.pieces.last().unwrap();

        assert!((4..=8).contains(&drawn), "seed {seed}: {drawn} pieces");
        assert_eq!(checkpoints, drawn - 1, "seed {seed}");
        assert!(matches!(last, PieceSpec::Finish { .. }), "seed {seed}");
    }
}

#[test]
fn every_jump_has_at_least_a_33_m_run_up_from_the_last_line() {
    let catalog = catalog();

    for seed in 0..SEEDS {
        let course = generate_course(&catalog, seed);
        let mut last_line_m = course.start.pos[0];
        for piece in &course.pieces {
            let x_m = piece.position()[0];
            match piece {
                PieceSpec::Checkpoint { .. } => last_line_m = x_m,
                PieceSpec::Ramp { .. } => {
                    assert!(
                        x_m - last_line_m >= 33.0 - 1e-3,
                        "seed {seed}: ramp at {x_m}"
                    );
                }
                PieceSpec::Wall { .. } => {
                    // The wall's own jump ramp starts 12 m before the wall.
                    assert!(
                        x_m - 12.0 - last_line_m >= 33.0 - 1e-3,
                        "seed {seed}: wall at {x_m}"
                    );
                }
                _ => {}
            }
        }
    }
}

#[test]
fn checkpoints_and_the_finish_sit_on_floor_not_over_a_gap() {
    let catalog = catalog();

    for seed in 0..SEEDS {
        let course = generate_course(&catalog, seed);
        let gaps: Vec<(f32, f32)> = course
            .pieces
            .iter()
            .filter_map(|piece| match piece {
                PieceSpec::Gap { pos, length } => Some((pos[0], pos[0] + length)),
                _ => None,
            })
            .collect();
        for piece in &course.pieces {
            if let PieceSpec::Checkpoint { pos, .. } | PieceSpec::Finish { pos, .. } = piece {
                let over_gap = gaps
                    .iter()
                    .any(|(from, to)| (*from..=*to).contains(&pos[0]));
                assert!(!over_gap, "seed {seed}: line at {} over a gap", pos[0]);
            }
        }
    }
}

/// A flip piece: a corkscrew, or a kick plate as strong as a flip plate (plain kick
/// plates push up at most 4000 N·s; the weakest flip preset 6500).
fn is_flip_piece(piece: &PieceSpec) -> bool {
    match piece {
        PieceSpec::Corkscrew { .. } => true,
        PieceSpec::KickPlate { impulse, .. } => impulse[1] >= 6000.0,
        _ => false,
    }
}

#[test]
fn most_generated_courses_have_a_piece_that_flips_the_car() {
    let catalog = catalog();

    let with_flip = (0..SEEDS)
        .filter(|seed| {
            let course = generate_course(&catalog, *seed);
            course.pieces.iter().any(is_flip_piece)
        })
        .count();

    let share = with_flip as f32 / SEEDS as f32;
    assert!(share >= 0.6, "only {share} of courses have a flip piece");
}

#[test]
fn a_flip_plate_without_presets_is_refused() {
    let broken = r#"{"version": 1, "start_height_m": 0.7, "track_half_width_m": 10.0,
        "bounds_margin_m": 30.0, "piece_count": [4, 8], "straight_m": [20.0, 40.0],
        "jump_run_up_m": 33.0, "pieces": [
          {"kind": "flip_plate", "weight": 1.0, "strength": [1.0, 1.1], "strong_strength": 3.0, "run_out_m": 45.0,
           "presets": []}]}"#;

    let error = PieceCatalog::from_json_str(broken).unwrap_err();

    assert!(error.to_string().contains("preset"), "{error}");
}

#[test]
fn a_broken_catalog_is_refused_with_a_message_naming_the_file() {
    let broken = DEFAULT_PIECES_JSON.replace("\"piece_count\": [4, 8]", "\"piece_count\": [8, 4]");

    let error = PieceCatalog::from_json_str(&broken).unwrap_err();

    assert!(error.to_string().contains("pieces.json"), "{error}");
    assert!(error.to_string().contains("piece_count"), "{error}");
}
