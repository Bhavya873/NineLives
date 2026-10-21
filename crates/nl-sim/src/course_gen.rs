//! The procedural course generator for Stage D training (SPEC §7.11): 4–8 pieces drawn
//! from `configs/pieces.json` with random sizes inside safe ranges, joined by straights,
//! a checkpoint after each piece's landing zone and a finish at the end. Courses run
//! straight along +X (gaps cut the floor along world X). The same seed always gives the
//! same course (ChaCha8 stream 4 of the episode seed), and every course it makes passes
//! `CourseSpec`'s checks.

use std::sync::OnceLock;

use rand::RngExt;
use rand_chacha::ChaCha8Rng;
use serde::Deserialize;

use crate::course::{CourseSpec, PieceSpec, Side};
use crate::course_build::{
    BUMP_SPACING_M, KICK_PLATE_LENGTH_M, WALL_RAMP_GAP_M, WALL_RAMP_LENGTH_M, WALL_THICKNESS_M,
};
use crate::driving_stages::{pick_weighted, straight_course, stream_rng};
use crate::scenarios::draw;

/// The repo's `configs/pieces.json`, built in like the Gauntlet.
pub const DEFAULT_PIECES_JSON: &str = include_str!("../../../configs/pieces.json");
/// The only pieces.json version this build reads.
const PIECES_SCHEMA_VERSION: u32 = 1;
/// The ChaCha8 stream for course draws.
const COURSE_RNG_STREAM: u64 = 4;

/// pieces.json: how generated courses are laid out and which pieces they use.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PieceCatalog {
    /// Must be 1.
    pub version: u32,
    /// A note for people reading the file; ignored.
    #[serde(default)]
    pub about: String,
    /// The car starts with its centre of mass this high, in m.
    pub start_height_m: f32,
    /// The track (and its bounds) reach this far either side of the centre line, in m.
    pub track_half_width_m: f32,
    /// The bounds reach this far behind the start and past the finish, in m.
    pub bounds_margin_m: f32,
    /// How many pieces, low and high inclusive.
    pub piece_count: [u32; 2],
    /// Straight floor before each piece, in m.
    pub straight_m: [f32; 2],
    /// Before a jump the straight is at least this long, so a car starting at rest
    /// (after a checkpoint respawn) reaches jumping speed, in m.
    pub jump_run_up_m: f32,
    /// The pieces to draw from.
    pub pieces: Vec<PieceTemplate>,
}

/// One kind of piece and the ranges its sizes are drawn from. `weight` is its relative
/// chance; `run_out_m` is flat floor kept after it, where the car lands.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PieceTemplate {
    /// A ramp to jump off (a jump).
    Ramp {
        weight: f32,
        length_m: [f32; 2],
        width_m: [f32; 2],
        angle_deg: [f32; 2],
        run_out_m: f32,
    },
    /// A ramp with a gap in the floor right after its lip (a jump).
    RampGap {
        weight: f32,
        length_m: [f32; 2],
        width_m: [f32; 2],
        angle_deg: [f32; 2],
        gap_m: [f32; 2],
        run_out_m: f32,
    },
    /// A kicker on a random side.
    Kicker {
        weight: f32,
        length_m: [f32; 2],
        width_m: [f32; 2],
        angle_deg: [f32; 2],
        run_out_m: f32,
    },
    /// A corkscrew on a random side: it flips the car, so the thrusters must save it
    /// (a jump).
    Corkscrew {
        weight: f32,
        length_m: [f32; 2],
        width_m: [f32; 2],
        angle_deg: [f32; 2],
        bank_deg: [f32; 2],
        run_out_m: f32,
    },
    /// A run of bumps.
    Bumps {
        weight: f32,
        count: [u32; 2],
        height_m: [f32; 2],
        run_out_m: f32,
    },
    /// A kick plate pushing up and to a random side, rolling the car the same way.
    KickPlate {
        weight: f32,
        impulse_up_n_s: [f32; 2],
        impulse_side_n_s: [f32; 2],
        angular_roll_n_m_s: [f32; 2],
        run_out_m: f32,
    },
    /// A halfpipe to drive through.
    Halfpipe {
        weight: f32,
        radius_m: [f32; 2],
        length_m: [f32; 2],
        run_out_m: f32,
    },
    /// A wall with its jump ramp (a jump).
    Wall {
        weight: f32,
        height_m: [f32; 2],
        run_out_m: f32,
    },
}

/// Why pieces.json could not be used.
#[derive(Debug, thiserror::Error)]
pub enum PieceCatalogError {
    #[error("pieces.json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("pieces.json: {0}")]
    Bad(String),
}

impl PieceCatalog {
    /// Reads and checks the text of a pieces.json.
    pub fn from_json_str(text: &str) -> Result<PieceCatalog, PieceCatalogError> {
        let catalog: PieceCatalog = serde_json::from_str(text)?;
        catalog.check()?;
        Ok(catalog)
    }

    /// The built-in pieces.json, read once.
    pub fn built_in() -> Result<&'static PieceCatalog, String> {
        static CATALOG: OnceLock<Result<PieceCatalog, String>> = OnceLock::new();
        CATALOG
            .get_or_init(|| {
                PieceCatalog::from_json_str(DEFAULT_PIECES_JSON).map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    /// Version, counts, ranges and weights must make sense; the courses themselves are
    /// checked by `CourseSpec`'s rules in the tests.
    fn check(&self) -> Result<(), PieceCatalogError> {
        let bad = |problem: &str| Err(PieceCatalogError::Bad(problem.to_string()));
        if self.version != PIECES_SCHEMA_VERSION {
            return bad("`version` must be 1");
        }
        let [fewest, most] = self.piece_count;
        if fewest == 0 || fewest > most {
            return bad("`piece_count` must be [low, high] with 1 ≤ low ≤ high");
        }
        let sizes = [
            self.start_height_m,
            self.track_half_width_m,
            self.bounds_margin_m,
            self.jump_run_up_m,
        ];
        if !sizes.iter().all(|size| size.is_finite() && *size > 0.0) {
            return bad("sizes must be finite numbers above zero");
        }
        if !is_range(self.straight_m) {
            return bad("`straight_m` must be [low, high] with 0 ≤ low ≤ high");
        }
        let mut total_weight = 0.0;
        for (index, piece) in self.pieces.iter().enumerate() {
            if !piece.has_sane_numbers() {
                return Err(PieceCatalogError::Bad(format!(
                    "piece {index}: weights and run-outs must be zero or more, and ranges [low, high] with 0 ≤ low ≤ high"
                )));
            }
            total_weight += piece.weight();
        }
        if total_weight <= 0.0 {
            return bad("at least one piece needs a weight above zero");
        }
        Ok(())
    }
}

impl PieceTemplate {
    /// Its relative chance of being drawn.
    fn weight(&self) -> f32 {
        match self {
            PieceTemplate::Ramp { weight, .. }
            | PieceTemplate::RampGap { weight, .. }
            | PieceTemplate::Kicker { weight, .. }
            | PieceTemplate::Corkscrew { weight, .. }
            | PieceTemplate::Bumps { weight, .. }
            | PieceTemplate::KickPlate { weight, .. }
            | PieceTemplate::Halfpipe { weight, .. }
            | PieceTemplate::Wall { weight, .. } => *weight,
        }
    }

    /// Flat floor kept after it, in m.
    fn run_out_m(&self) -> f32 {
        match self {
            PieceTemplate::Ramp { run_out_m, .. }
            | PieceTemplate::RampGap { run_out_m, .. }
            | PieceTemplate::Kicker { run_out_m, .. }
            | PieceTemplate::Corkscrew { run_out_m, .. }
            | PieceTemplate::Bumps { run_out_m, .. }
            | PieceTemplate::KickPlate { run_out_m, .. }
            | PieceTemplate::Halfpipe { run_out_m, .. }
            | PieceTemplate::Wall { run_out_m, .. } => *run_out_m,
        }
    }

    /// True for pieces that throw the car into the air, which need a long run-up.
    fn is_jump(&self) -> bool {
        matches!(
            self,
            PieceTemplate::Ramp { .. }
                | PieceTemplate::RampGap { .. }
                | PieceTemplate::Corkscrew { .. }
                | PieceTemplate::Wall { .. }
        )
    }

    /// Weight and run-out are finite and not negative; every range is in order.
    fn has_sane_numbers(&self) -> bool {
        let singles = [self.weight(), self.run_out_m()];
        if !singles
            .iter()
            .all(|value| value.is_finite() && *value >= 0.0)
        {
            return false;
        }
        let ranges: Vec<[f32; 2]> = match self {
            PieceTemplate::Ramp {
                length_m,
                width_m,
                angle_deg,
                ..
            }
            | PieceTemplate::Kicker {
                length_m,
                width_m,
                angle_deg,
                ..
            } => vec![*length_m, *width_m, *angle_deg],
            PieceTemplate::RampGap {
                length_m,
                width_m,
                angle_deg,
                gap_m,
                ..
            } => vec![*length_m, *width_m, *angle_deg, *gap_m],
            PieceTemplate::Corkscrew {
                length_m,
                width_m,
                angle_deg,
                bank_deg,
                ..
            } => vec![*length_m, *width_m, *angle_deg, *bank_deg],
            PieceTemplate::Bumps {
                count, height_m, ..
            } => vec![[count[0] as f32, count[1] as f32], *height_m],
            PieceTemplate::KickPlate {
                impulse_up_n_s,
                impulse_side_n_s,
                angular_roll_n_m_s,
                ..
            } => vec![*impulse_up_n_s, *impulse_side_n_s, *angular_roll_n_m_s],
            PieceTemplate::Halfpipe {
                radius_m, length_m, ..
            } => vec![*radius_m, *length_m],
            PieceTemplate::Wall { height_m, .. } => vec![*height_m],
        };
        ranges.into_iter().all(is_range)
    }
}

/// The course for the episode with `seed`. Layout along +X: start, then for each piece
/// a straight, the piece and its run-out; a checkpoint after every run-out but the
/// last, where the finish goes instead.
pub fn generate_course(catalog: &PieceCatalog, seed: u64) -> CourseSpec {
    let mut rng = stream_rng(seed, COURSE_RNG_STREAM);
    let [fewest, most] = catalog.piece_count;
    let count = rng.random_range(fewest..=most);
    let weights: Vec<f32> = catalog.pieces.iter().map(PieceTemplate::weight).collect();
    let indices: Vec<usize> = (0..catalog.pieces.len()).collect();
    let mut pieces = Vec::new();
    let mut x_m = 0.0;
    for section in 0..count {
        let template = &catalog.pieces[pick_weighted(&indices, &weights, &mut rng)];
        let mut straight_m = draw(&mut rng, catalog.straight_m);
        if template.is_jump() {
            straight_m = straight_m.max(catalog.jump_run_up_m);
        }
        x_m += straight_m;
        let (laid, length_m) = lay_piece(template, x_m, &mut rng);
        pieces.extend(laid);
        x_m += length_m + template.run_out_m();
        let line = [x_m, 0.0, 0.0];
        if section + 1 < count {
            pieces.push(PieceSpec::Checkpoint {
                pos: line,
                yaw: 0.0,
            });
        } else {
            pieces.push(PieceSpec::Finish {
                pos: line,
                yaw: 0.0,
            });
        }
    }
    let name = format!("Generated {seed}");
    straight_course(&name, catalog, x_m, pieces)
}

/// The course pieces for one template starting at `x_m`, and how far along the course
/// they reach from there, in m.
fn lay_piece(template: &PieceTemplate, x_m: f32, rng: &mut ChaCha8Rng) -> (Vec<PieceSpec>, f32) {
    let pos = [x_m, 0.0, 0.0];
    match template {
        PieceTemplate::Ramp {
            length_m,
            width_m,
            angle_deg,
            ..
        } => {
            let length = draw(rng, *length_m);
            let ramp = ramp(pos, length, draw(rng, *width_m), draw(rng, *angle_deg));
            (vec![ramp], length)
        }
        PieceTemplate::RampGap {
            length_m,
            width_m,
            angle_deg,
            gap_m,
            ..
        } => {
            let length = draw(rng, *length_m);
            let ramp = ramp(pos, length, draw(rng, *width_m), draw(rng, *angle_deg));
            let gap_length = draw(rng, *gap_m);
            let gap = PieceSpec::Gap {
                pos: [x_m + length, 0.0, 0.0],
                length: gap_length,
            };
            (vec![ramp, gap], length + gap_length)
        }
        PieceTemplate::Kicker {
            length_m,
            width_m,
            angle_deg,
            ..
        } => {
            let length = draw(rng, *length_m);
            let kicker = PieceSpec::Kicker {
                pos,
                yaw: 0.0,
                length,
                width: draw(rng, *width_m),
                angle_deg: draw(rng, *angle_deg),
                side: random_side(rng),
            };
            (vec![kicker], length)
        }
        PieceTemplate::Corkscrew {
            length_m,
            width_m,
            angle_deg,
            bank_deg,
            ..
        } => {
            let length = draw(rng, *length_m);
            let corkscrew = PieceSpec::Corkscrew {
                pos,
                yaw: 0.0,
                length,
                width: draw(rng, *width_m),
                angle_deg: draw(rng, *angle_deg),
                bank_deg: draw(rng, *bank_deg),
                side: random_side(rng),
            };
            (vec![corkscrew], length)
        }
        PieceTemplate::Bumps {
            count, height_m, ..
        } => {
            let count = rng.random_range(count[0]..=count[1]);
            let bumps = PieceSpec::Bumps {
                pos,
                yaw: 0.0,
                count,
                height: draw(rng, *height_m),
            };
            (vec![bumps], count as f32 * BUMP_SPACING_M)
        }
        PieceTemplate::KickPlate {
            impulse_up_n_s,
            impulse_side_n_s,
            angular_roll_n_m_s,
            ..
        } => (
            vec![kick_plate(
                pos,
                rng,
                *impulse_up_n_s,
                *impulse_side_n_s,
                *angular_roll_n_m_s,
            )],
            KICK_PLATE_LENGTH_M,
        ),
        PieceTemplate::Halfpipe {
            radius_m, length_m, ..
        } => {
            let length = draw(rng, *length_m);
            let halfpipe = PieceSpec::Halfpipe {
                pos,
                yaw: 0.0,
                radius: draw(rng, *radius_m),
                length,
            };
            (vec![halfpipe], length)
        }
        PieceTemplate::Wall { height_m, .. } => {
            // A wall's jump ramp starts this far before the wall itself.
            let ramp_m = WALL_RAMP_LENGTH_M + WALL_RAMP_GAP_M;
            let wall = PieceSpec::Wall {
                pos: [x_m + ramp_m, 0.0, 0.0],
                yaw: 0.0,
                height: draw(rng, *height_m),
                with_ramp: true,
            };
            (vec![wall], ramp_m + WALL_THICKNESS_M)
        }
    }
}

/// A straight ramp piece.
fn ramp(pos: [f32; 3], length: f32, width: f32, angle_deg: f32) -> PieceSpec {
    PieceSpec::Ramp {
        pos,
        yaw: 0.0,
        length,
        width,
        angle_deg,
    }
}

/// A kick plate pushing up and to one side (+Z or −Z) with a roll the same way, as the
/// Gauntlet's plates do.
fn kick_plate(
    pos: [f32; 3],
    rng: &mut ChaCha8Rng,
    up_n_s: [f32; 2],
    side_n_s: [f32; 2],
    roll_n_m_s: [f32; 2],
) -> PieceSpec {
    let sign = if coin_flip(rng) { 1.0 } else { -1.0 };
    PieceSpec::KickPlate {
        pos,
        yaw: 0.0,
        impulse: [0.0, draw(rng, up_n_s), sign * draw(rng, side_n_s)],
        angular: [sign * draw(rng, roll_n_m_s), 0.0, 0.0],
    }
}

/// Left or right, evenly.
fn random_side(rng: &mut ChaCha8Rng) -> Side {
    if coin_flip(rng) {
        Side::Left
    } else {
        Side::Right
    }
}

/// Heads or tails.
fn coin_flip(rng: &mut ChaCha8Rng) -> bool {
    rng.random_range(0.0..1.0) < 0.5
}

/// A [low, high] pair, finite, in order and not negative.
fn is_range(range: [f32; 2]) -> bool {
    let [low, high] = range;
    low.is_finite() && high.is_finite() && 0.0 <= low && low <= high
}
