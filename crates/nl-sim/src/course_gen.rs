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
    KICK_PLATE_LENGTH_M, bump_spacing_m, ramp_lip_height_m, wall_ramp_total_length_m,
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
    /// The arc maths the pieces are laid out by (docs/contracts/ramp-curvature.md).
    pub design: DesignRules,
    /// The pieces to draw from.
    pub pieces: Vec<PieceTemplate>,
}

/// The numbers behind the piece geometry and spacing (pieces.json `design`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesignRules {
    /// The fastest clean entry the pieces are built for, in m/s (200 km/h).
    pub speed_m_s: f32,
    /// Smallest radius of a concave entry curve, in m (equals
    /// `course_build::MIN_CONCAVE_RADIUS_M`; a test keeps them in step).
    pub min_concave_radius_m: f32,
    /// Smallest radius of a crest in the middle of a piece, in m: v²/g at the design
    /// speed, so the car never leaves the ground by accident.
    pub min_crest_radius_m: f32,
    /// Largest angle between two neighbouring surface segments, in degrees.
    pub max_kink_deg: f32,
    /// The car's mass, in kg, for the flight of a flip plate's push.
    pub car_mass_kg: f32,
    /// Flat floor kept past the farthest landing at the design speed, in m.
    pub landing_margin_m: f32,
}

/// One kind of piece and the ranges its sizes are drawn from. `weight` is its relative
/// chance; `run_out_m` is flat floor kept after it, where the car lands.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PieceTemplate {
    /// A ramp to jump off (a jump).
    Ramp {
        weight: f32,
        /// Radius of the entry curve; the ramp is `radius · angle` long.
        radius_m: [f32; 2],
        width_m: [f32; 2],
        angle_deg: [f32; 2],
        run_out_m: f32,
        /// The slowest clean entry that completes the piece, in m/s (0: any speed).
        v_complete_min_m_s: f32,
    },
    /// A ramp with a gap in the floor right after its lip (a jump).
    RampGap {
        weight: f32,
        radius_m: [f32; 2],
        width_m: [f32; 2],
        angle_deg: [f32; 2],
        gap_m: [f32; 2],
        /// How deep the catch pit in the gap is, in m.
        pit_depth_m: f32,
        run_out_m: f32,
        /// A car entering slower than this drops into the catch pit and drives out.
        v_complete_min_m_s: f32,
    },
    /// A kicker on a random side.
    Kicker {
        weight: f32,
        radius_m: [f32; 2],
        width_m: [f32; 2],
        angle_deg: [f32; 2],
        run_out_m: f32,
        v_complete_min_m_s: f32,
    },
    /// A corkscrew on a random side: it flips the car, so the thrusters must save it
    /// (a jump).
    Corkscrew {
        weight: f32,
        /// Radius of the low edge's entry curve; the piece is `radius · tan(angle)` long.
        radius_m: [f32; 2],
        width_m: [f32; 2],
        angle_deg: [f32; 2],
        bank_deg: [f32; 2],
        run_out_m: f32,
        v_complete_min_m_s: f32,
    },
    /// A run of bumps.
    Bumps {
        weight: f32,
        count: [u32; 2],
        height_m: [f32; 2],
        run_out_m: f32,
        v_complete_min_m_s: f32,
    },
    /// A kick plate pushing up and to a random side, rolling the car the same way.
    KickPlate {
        weight: f32,
        impulse_up_n_s: [f32; 2],
        impulse_side_n_s: [f32; 2],
        angular_roll_n_m_s: [f32; 2],
        run_out_m: f32,
        v_complete_min_m_s: f32,
    },
    /// A flip plate: a kick plate with one of the named `presets`, its impulses scaled by
    /// a drawn `strength`. It throws the car into a front or back flip, a barrel roll, a
    /// flat spin or onto its roof, so the thrusters must save it (a jump).
    /// `strong_strength` is not drawn here: hand-made courses and Stage F use it for
    /// flips the thrusters can't stop before the car is upside down.
    FlipPlate {
        weight: f32,
        strength: [f32; 2],
        strong_strength: f32,
        presets: Vec<FlipPreset>,
        run_out_m: f32,
        v_complete_min_m_s: f32,
    },
    /// A halfpipe to drive through.
    Halfpipe {
        weight: f32,
        radius_m: [f32; 2],
        length_m: [f32; 2],
        run_out_m: f32,
        v_complete_min_m_s: f32,
    },
    /// A wall with its jump ramp (a jump).
    Wall {
        weight: f32,
        height_m: [f32; 2],
        run_out_m: f32,
        v_complete_min_m_s: f32,
    },
}

/// One named flip-plate kick. Both impulses are in the world frame, so they suit a course
/// running along +X, as every generated course does.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlipPreset {
    /// What the kick does, e.g. "front_flip".
    pub name: String,
    /// Its relative chance on a generated course; 0 keeps it for hand-made courses.
    pub weight: f32,
    /// Linear impulse, in N·s.
    pub impulse_n_s: [f32; 3],
    /// Angular impulse, in N·m·s (X rolls right, Y yaws left, Z pitches the nose up).
    pub angular_n_m_s: [f32; 3],
}

/// One of `presets`, drawn by weight (Stage F and generated flip plates share it).
pub(crate) fn pick_preset<'a>(presets: &'a [FlipPreset], rng: &mut ChaCha8Rng) -> &'a FlipPreset {
    let weights: Vec<f32> = presets.iter().map(|preset| preset.weight).collect();
    let indices: Vec<usize> = (0..presets.len()).collect();
    &presets[pick_weighted(&indices, &weights, rng)]
}

impl FlipPreset {
    /// Every impulse is a finite number and the weight is finite and not negative.
    fn is_sane(&self) -> bool {
        let values = self.impulse_n_s.iter().chain(&self.angular_n_m_s);
        let impulses_finite = values.into_iter().all(|value| value.is_finite());
        impulses_finite && self.weight.is_finite() && self.weight >= 0.0
    }

    /// The kick plate piece at `pos` with this preset's impulses times `strength`.
    pub fn kick_plate_at(&self, pos: [f32; 3], strength: f32) -> PieceSpec {
        PieceSpec::KickPlate {
            pos,
            yaw: 0.0,
            impulse: self.impulse_n_s.map(|value| value * strength),
            angular: self.angular_n_m_s.map(|value| value * strength),
        }
    }
}

/// The flip plate settings from pieces.json, for tests, courses and Stage F.
#[derive(Debug, Clone, PartialEq)]
pub struct FlipPlates {
    /// Every named kick.
    pub presets: Vec<FlipPreset>,
    /// The strength range generated courses draw from (mild: the thrusters stop the flip).
    pub strength: [f32; 2],
    /// The strength of a strong plate, which turns the car over even with the thrusters on.
    pub strong_strength: f32,
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

    /// The first flip plate template's presets and strengths, if there is one.
    pub fn flip_plates(&self) -> Option<FlipPlates> {
        for piece in &self.pieces {
            if let PieceTemplate::FlipPlate {
                strength,
                strong_strength,
                presets,
                ..
            } = piece
            {
                return Some(FlipPlates {
                    presets: presets.clone(),
                    strength: *strength,
                    strong_strength: *strong_strength,
                });
            }
        }
        None
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
        self.check_design_numbers()?;
        if !is_range(self.straight_m) {
            return bad("`straight_m` must be [low, high] with 0 ≤ low ≤ high");
        }
        self.check_pieces()?;
        Ok(())
    }

    /// The design rules must be finite numbers above zero.
    fn check_design_numbers(&self) -> Result<(), PieceCatalogError> {
        let design = &self.design;
        let design_numbers = [
            design.speed_m_s,
            design.min_concave_radius_m,
            design.min_crest_radius_m,
            design.max_kink_deg,
            design.car_mass_kg,
            design.landing_margin_m,
        ];
        if !design_numbers
            .iter()
            .all(|value| value.is_finite() && *value > 0.0)
        {
            return Err(PieceCatalogError::Bad(
                "`design` numbers must be finite and above zero".to_string(),
            ));
        }
        Ok(())
    }

    /// Every piece template must have sane numbers, and some weight must be above zero.
    fn check_pieces(&self) -> Result<(), PieceCatalogError> {
        let mut total_weight = 0.0;
        for (index, piece) in self.pieces.iter().enumerate() {
            if !piece.has_sane_numbers() {
                return Err(PieceCatalogError::Bad(format!(
                    "piece {index}: weights and run-outs must be zero or more, ranges [low, high] with 0 ≤ low ≤ high, and a flip plate needs presets with finite impulses and at least one weight above zero"
                )));
            }
            total_weight += piece.weight();
        }
        if total_weight <= 0.0 {
            return Err(PieceCatalogError::Bad(
                "at least one piece needs a weight above zero".to_string(),
            ));
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
            | PieceTemplate::FlipPlate { weight, .. }
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
            | PieceTemplate::FlipPlate { run_out_m, .. }
            | PieceTemplate::Halfpipe { run_out_m, .. }
            | PieceTemplate::Wall { run_out_m, .. } => *run_out_m,
        }
    }

    /// The slowest clean entry that completes the piece, in m/s (0: any speed).
    pub fn v_complete_min_m_s(&self) -> f32 {
        match self {
            PieceTemplate::Ramp {
                v_complete_min_m_s, ..
            }
            | PieceTemplate::RampGap {
                v_complete_min_m_s, ..
            }
            | PieceTemplate::Kicker {
                v_complete_min_m_s, ..
            }
            | PieceTemplate::Corkscrew {
                v_complete_min_m_s, ..
            }
            | PieceTemplate::Bumps {
                v_complete_min_m_s, ..
            }
            | PieceTemplate::KickPlate {
                v_complete_min_m_s, ..
            }
            | PieceTemplate::FlipPlate {
                v_complete_min_m_s, ..
            }
            | PieceTemplate::Halfpipe {
                v_complete_min_m_s, ..
            }
            | PieceTemplate::Wall {
                v_complete_min_m_s, ..
            } => *v_complete_min_m_s,
        }
    }

    /// True for pieces that throw the car into the air, which need a long run-up.
    fn is_jump(&self) -> bool {
        matches!(
            self,
            PieceTemplate::Ramp { .. }
                | PieceTemplate::RampGap { .. }
                | PieceTemplate::Corkscrew { .. }
                | PieceTemplate::FlipPlate { .. }
                | PieceTemplate::Wall { .. }
        )
    }

    /// Weight and run-out are finite and not negative; every range is in order.
    fn has_sane_numbers(&self) -> bool {
        let singles = [self.weight(), self.run_out_m(), self.v_complete_min_m_s()];
        if !singles
            .iter()
            .all(|value| value.is_finite() && *value >= 0.0)
        {
            return false;
        }
        if !self.has_sane_flip_presets() {
            return false;
        }
        let mut ranges = self.ramp_family_ranges();
        ranges.extend(self.other_ranges());
        ranges.into_iter().all(is_range)
    }

    /// A flip plate needs sane presets with a total weight above zero; every other piece
    /// has no presets to check.
    fn has_sane_flip_presets(&self) -> bool {
        if let PieceTemplate::FlipPlate { presets, .. } = self {
            let total_weight: f32 = presets.iter().map(|preset| preset.weight).sum();
            return presets.iter().all(FlipPreset::is_sane) && total_weight > 0.0;
        }
        true
    }

    /// The [low, high] ranges of the ramp-like pieces (empty for every other piece).
    fn ramp_family_ranges(&self) -> Vec<[f32; 2]> {
        match self {
            PieceTemplate::Ramp {
                radius_m,
                width_m,
                angle_deg,
                ..
            }
            | PieceTemplate::Kicker {
                radius_m,
                width_m,
                angle_deg,
                ..
            } => vec![*radius_m, *width_m, *angle_deg],
            PieceTemplate::RampGap {
                radius_m,
                width_m,
                angle_deg,
                gap_m,
                ..
            } => vec![*radius_m, *width_m, *angle_deg, *gap_m],
            PieceTemplate::Corkscrew {
                radius_m,
                width_m,
                angle_deg,
                bank_deg,
                ..
            } => vec![*radius_m, *width_m, *angle_deg, *bank_deg],
            _ => Vec::new(),
        }
    }

    /// The [low, high] ranges of the pieces that are not ramp-like (empty for the ramps).
    fn other_ranges(&self) -> Vec<[f32; 2]> {
        match self {
            PieceTemplate::Bumps {
                count, height_m, ..
            } => vec![[count[0] as f32, count[1] as f32], *height_m],
            PieceTemplate::KickPlate {
                impulse_up_n_s,
                impulse_side_n_s,
                angular_roll_n_m_s,
                ..
            } => vec![*impulse_up_n_s, *impulse_side_n_s, *angular_roll_n_m_s],
            PieceTemplate::FlipPlate {
                strength,
                strong_strength,
                ..
            } => vec![*strength, [*strong_strength, *strong_strength]],
            PieceTemplate::Halfpipe {
                radius_m, length_m, ..
            } => vec![*radius_m, *length_m],
            PieceTemplate::Wall { height_m, .. } => vec![*height_m],
            _ => Vec::new(),
        }
    }
}

/// The course for the episode with `seed`. Layout along +X: start, then for each piece
/// a straight, the piece and its run-out; a checkpoint after every run-out but the
/// last, where the finish goes instead. A run-out is at least as long as the farthest
/// a car at the design speed can land past the piece (`landing_reach_m`), so a clean
/// car never lands on the next piece.
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
        let laid = lay_piece(template, &catalog.design, x_m, &mut rng);
        x_m += laid.length_m + laid.run_out_m(template.run_out_m(), &catalog.design);
        pieces.extend(laid.pieces);
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

/// One laid piece: its course pieces, how far it reaches along the course and how far
/// past its start a car at the design speed can land.
struct LaidPiece {
    pieces: Vec<PieceSpec>,
    /// How far along the course the piece reaches from its start, in m.
    length_m: f32,
    /// The farthest a car at the design speed lands from the piece's start, in m
    /// (0 for pieces that do not throw the car).
    landing_reach_m: f32,
}

impl LaidPiece {
    fn on_the_ground(pieces: Vec<PieceSpec>, length_m: f32) -> LaidPiece {
        LaidPiece {
            pieces,
            length_m,
            landing_reach_m: 0.0,
        }
    }

    /// The flat floor after the piece: the template's own run-out, or enough to cover the
    /// farthest landing plus the margin, whichever is longer.
    fn run_out_m(&self, template_run_out_m: f32, design: &DesignRules) -> f32 {
        let needed_m = self.landing_reach_m + design.landing_margin_m - self.length_m;
        template_run_out_m.max(needed_m)
    }
}

/// How far a car leaving a lip at `speed_m_s`, `angle_deg` above level and `height_m`
/// above the floor lands from the lip, in m (ballistic flight, no drag).
pub fn flight_distance_m(speed_m_s: f32, angle_deg: f32, height_m: f32) -> f32 {
    let gravity = 9.81;
    let angle_rad = angle_deg.to_radians();
    let rise_m_s = speed_m_s * libm::sinf(angle_rad);
    let fall_time_s =
        (rise_m_s + libm::sqrtf(rise_m_s * rise_m_s + 2.0 * gravity * height_m)) / gravity;
    speed_m_s * libm::cosf(angle_rad) * fall_time_s
}

/// The course pieces for one template starting at `x_m`.
fn lay_piece(
    template: &PieceTemplate,
    design: &DesignRules,
    x_m: f32,
    rng: &mut ChaCha8Rng,
) -> LaidPiece {
    match template {
        PieceTemplate::Ramp { .. } => lay_ramp(template, design, x_m, rng),
        PieceTemplate::RampGap { .. } => lay_ramp_gap(template, design, x_m, rng),
        PieceTemplate::Kicker { .. } => lay_kicker(template, design, x_m, rng),
        PieceTemplate::Corkscrew { .. } => lay_corkscrew(template, design, x_m, rng),
        PieceTemplate::Bumps { .. } => lay_bumps(template, x_m, rng),
        PieceTemplate::KickPlate { .. } => lay_kick_plate(template, x_m, rng),
        PieceTemplate::FlipPlate { .. } => lay_flip_plate(template, design, x_m, rng),
        PieceTemplate::Halfpipe { .. } => lay_halfpipe(template, x_m, rng),
        PieceTemplate::Wall { .. } => lay_wall(template, x_m, rng),
    }
}

/// An empty piece. The `lay_*` helpers return it only if `lay_piece` hands them the
/// wrong kind of template, which its `match` rules out.
fn no_piece() -> LaidPiece {
    LaidPiece::on_the_ground(Vec::new(), 0.0)
}

/// A ramp template laid at `x_m`.
fn lay_ramp(
    template: &PieceTemplate,
    design: &DesignRules,
    x_m: f32,
    rng: &mut ChaCha8Rng,
) -> LaidPiece {
    let PieceTemplate::Ramp {
        radius_m,
        width_m,
        angle_deg,
        v_complete_min_m_s,
        ..
    } = template
    else {
        return no_piece();
    };
    let pos = [x_m, 0.0, 0.0];
    let speed_m_s = design.speed_m_s;
    let angle = draw(rng, *angle_deg);
    let length = draw(rng, *radius_m) * angle.to_radians();
    let width = draw(rng, *width_m);
    let ramp = ramp(pos, length, width, angle, *v_complete_min_m_s);
    let lip_m = ramp_lip_height_m(length, angle);
    LaidPiece {
        pieces: vec![ramp],
        length_m: length,
        landing_reach_m: length + flight_distance_m(speed_m_s, angle, lip_m),
    }
}

/// A ramp-with-a-gap template laid at `x_m`.
fn lay_ramp_gap(
    template: &PieceTemplate,
    design: &DesignRules,
    x_m: f32,
    rng: &mut ChaCha8Rng,
) -> LaidPiece {
    let PieceTemplate::RampGap {
        radius_m,
        width_m,
        angle_deg,
        gap_m,
        pit_depth_m,
        v_complete_min_m_s,
        ..
    } = template
    else {
        return no_piece();
    };
    let pos = [x_m, 0.0, 0.0];
    let speed_m_s = design.speed_m_s;
    let angle = draw(rng, *angle_deg);
    let length = draw(rng, *radius_m) * angle.to_radians();
    let width = draw(rng, *width_m);
    let ramp = ramp(pos, length, width, angle, *v_complete_min_m_s);
    let lip_m = ramp_lip_height_m(length, angle);
    let gap_length = draw(rng, *gap_m);
    let gap = PieceSpec::Gap {
        pos: [x_m + length, 0.0, 0.0],
        length: gap_length,
        pit_depth: *pit_depth_m,
        entry_height: lip_m,
    };
    LaidPiece {
        pieces: vec![ramp, gap],
        length_m: length + gap_length,
        landing_reach_m: length + flight_distance_m(speed_m_s, angle, lip_m),
    }
}

/// A kicker template laid at `x_m`.
fn lay_kicker(
    template: &PieceTemplate,
    design: &DesignRules,
    x_m: f32,
    rng: &mut ChaCha8Rng,
) -> LaidPiece {
    let PieceTemplate::Kicker {
        radius_m,
        width_m,
        angle_deg,
        ..
    } = template
    else {
        return no_piece();
    };
    let pos = [x_m, 0.0, 0.0];
    let speed_m_s = design.speed_m_s;
    let angle = draw(rng, *angle_deg);
    let length = draw(rng, *radius_m) * angle.to_radians();
    let kicker = PieceSpec::Kicker {
        pos,
        yaw: 0.0,
        length,
        width: draw(rng, *width_m),
        angle_deg: angle,
        side: random_side(rng),
    };
    let lip_m = ramp_lip_height_m(length, angle);
    LaidPiece {
        pieces: vec![kicker],
        length_m: length,
        landing_reach_m: length + flight_distance_m(speed_m_s, angle, lip_m),
    }
}

/// A corkscrew template laid at `x_m`.
fn lay_corkscrew(
    template: &PieceTemplate,
    design: &DesignRules,
    x_m: f32,
    rng: &mut ChaCha8Rng,
) -> LaidPiece {
    let PieceTemplate::Corkscrew {
        radius_m,
        width_m,
        angle_deg,
        bank_deg,
        ..
    } = template
    else {
        return no_piece();
    };
    let pos = [x_m, 0.0, 0.0];
    let speed_m_s = design.speed_m_s;
    let angle = draw(rng, *angle_deg);
    // The low edge is a parabola: radius at the foot = length / tan(angle).
    let length = draw(rng, *radius_m) * libm::tanf(angle.to_radians());
    let corkscrew = PieceSpec::Corkscrew {
        pos,
        yaw: 0.0,
        length,
        width: draw(rng, *width_m),
        angle_deg: angle,
        bank_deg: draw(rng, *bank_deg),
        side: random_side(rng),
    };
    let lip_m = 0.5 * length * libm::tanf(angle.to_radians());
    LaidPiece {
        pieces: vec![corkscrew],
        length_m: length,
        landing_reach_m: length + flight_distance_m(speed_m_s, angle, lip_m),
    }
}

/// A run-of-bumps template laid at `x_m`.
fn lay_bumps(template: &PieceTemplate, x_m: f32, rng: &mut ChaCha8Rng) -> LaidPiece {
    let PieceTemplate::Bumps {
        count, height_m, ..
    } = template
    else {
        return no_piece();
    };
    let pos = [x_m, 0.0, 0.0];
    let count = rng.random_range(count[0]..=count[1]);
    let height = draw(rng, *height_m);
    let bumps = PieceSpec::Bumps {
        pos,
        yaw: 0.0,
        count,
        height,
    };
    LaidPiece::on_the_ground(vec![bumps], count as f32 * bump_spacing_m(height))
}

/// A kick-plate template laid at `x_m`.
fn lay_kick_plate(template: &PieceTemplate, x_m: f32, rng: &mut ChaCha8Rng) -> LaidPiece {
    let PieceTemplate::KickPlate {
        impulse_up_n_s,
        impulse_side_n_s,
        angular_roll_n_m_s,
        ..
    } = template
    else {
        return no_piece();
    };
    let pos = [x_m, 0.0, 0.0];
    LaidPiece::on_the_ground(
        vec![kick_plate(
            pos,
            rng,
            *impulse_up_n_s,
            *impulse_side_n_s,
            *angular_roll_n_m_s,
        )],
        KICK_PLATE_LENGTH_M,
    )
}

/// A flip-plate template laid at `x_m`.
fn lay_flip_plate(
    template: &PieceTemplate,
    design: &DesignRules,
    x_m: f32,
    rng: &mut ChaCha8Rng,
) -> LaidPiece {
    let PieceTemplate::FlipPlate {
        strength, presets, ..
    } = template
    else {
        return no_piece();
    };
    let pos = [x_m, 0.0, 0.0];
    let speed_m_s = design.speed_m_s;
    let preset = pick_preset(presets, rng);
    let strength = draw(rng, *strength);
    let plate = preset.kick_plate_at(pos, strength);
    // The push throws the car up at impulse / mass, then it falls back.
    let rise_m_s = preset.impulse_n_s[1] * strength / design.car_mass_kg;
    let airtime_s = 2.0 * rise_m_s / 9.81;
    LaidPiece {
        pieces: vec![plate],
        length_m: KICK_PLATE_LENGTH_M,
        landing_reach_m: KICK_PLATE_LENGTH_M + speed_m_s * airtime_s,
    }
}

/// A halfpipe template laid at `x_m`.
fn lay_halfpipe(template: &PieceTemplate, x_m: f32, rng: &mut ChaCha8Rng) -> LaidPiece {
    let PieceTemplate::Halfpipe {
        radius_m, length_m, ..
    } = template
    else {
        return no_piece();
    };
    let pos = [x_m, 0.0, 0.0];
    let length = draw(rng, *length_m);
    let halfpipe = PieceSpec::Halfpipe {
        pos,
        yaw: 0.0,
        radius: draw(rng, *radius_m),
        length,
    };
    LaidPiece::on_the_ground(vec![halfpipe], length)
}

/// A wall-with-a-ramp template laid at `x_m`.
fn lay_wall(template: &PieceTemplate, x_m: f32, rng: &mut ChaCha8Rng) -> LaidPiece {
    let PieceTemplate::Wall { height_m, .. } = template else {
        return no_piece();
    };
    let pos = [x_m, 0.0, 0.0];
    let height = draw(rng, *height_m);
    let wall = PieceSpec::Wall {
        pos,
        yaw: 0.0,
        height,
        with_ramp: true,
    };
    let length = wall_ramp_total_length_m(height);
    LaidPiece::on_the_ground(vec![wall], length)
}

/// A straight ramp piece.
fn ramp(
    pos: [f32; 3],
    length: f32,
    width: f32,
    angle_deg: f32,
    v_complete_min_m_s: f32,
) -> PieceSpec {
    PieceSpec::Ramp {
        pos,
        yaw: 0.0,
        length,
        width,
        angle_deg,
        v_complete_min_m_s,
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
