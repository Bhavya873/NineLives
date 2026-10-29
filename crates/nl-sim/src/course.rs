//! Course files (SPEC §7.11, schema version 1): reading and checking a course's JSON.
//!
//! A course is a start pose, a bounding box and a list of pieces (ramps, gaps, car rows…)
//! laid along world +X. This module only reads and checks the file; `course_build`
//! turns the pieces into colliders. Course files come from outside the sim (the repo, a
//! generator, later maybe players), so every number is checked and every error names
//! the piece and field to fix.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The demo course (SPEC §7.11), built into the binary so the game and tests share it.
pub const GAUNTLET_JSON: &str = include_str!("../../../courses/gauntlet.json");
/// The training viewer's obstacle arena (SPEC §11.6): ramps and blocks around an open
/// drop zone at the origin, where Stage A drops start.
pub const ARENA_JSON: &str = include_str!("../../../courses/arena.json");
/// The game's open world for free driving (SPEC §11.6): zones of the pieces the AI
/// trains on (a jump boulevard, a skate park of halfpipes, a jump field, a bumps strip)
/// around a clear spawn at the origin, with open floor between them.
pub const PLAYGROUND_JSON: &str = include_str!("../../../courses/playground.json");

/// The only course schema version this build reads.
pub const COURSE_SCHEMA_VERSION: u32 = 1;
/// Most pieces a course may have; a guard against huge or hostile files.
const MAX_PIECES: usize = 200;
/// Most items one piece may repeat (cars in a row, bumps).
const MAX_COUNT: u32 = 20;
/// Smallest and largest size any piece may have along any axis, in m. The minimum keeps
/// every hull solid enough for Rapier to build.
const MIN_PIECE_SIZE_M: f32 = 0.1;
const MAX_PIECE_SIZE_M: f32 = 500.0;
/// Ramp, kicker and corkscrew angles must lie in [MIN, MAX), in degrees.
const MIN_RAMP_ANGLE_DEG: f32 = 1.0;
const MAX_RAMP_ANGLE_DEG: f32 = 60.0;
/// A corkscrew's final bank must lie in [MIN, MAX), in degrees: below 90°, so its
/// raised edge stays a slope and never folds over.
const MIN_BANK_DEG: f32 = 1.0;
const MAX_BANK_DEG: f32 = 80.0;
/// Largest `v_complete_min_m_s` a piece may publish, in m/s (above the fastest car).
const MAX_V_COMPLETE_MIN_M_S: f32 = 150.0;
/// Deepest catch pit, in m.
const MAX_PIT_DEPTH_M: f32 = 5.0;
/// Farthest any bounds corner may sit from the origin on any axis, in m. The floor
/// spans the bounds, and Rapier loses precision on huge boxes.
const MAX_BOUNDS_M: f32 = 5000.0;

/// The course's `start`: where the car begins.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartSpec {
    /// Centre-of-mass position, in m.
    pub pos: [f32; 3],
    /// Heading about +Y, in degrees (0 faces +X, the course direction).
    #[serde(default)]
    pub yaw: f32,
}

/// The course's `bounds`: leaving this box is an out-of-bounds crash (SPEC §7.9).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundsSpec {
    /// Lowest corner, in m.
    pub min: [f32; 3],
    /// Highest corner, in m.
    pub max: [f32; 3],
}

impl BoundsSpec {
    /// True when `point` (m) lies inside the box, edges included.
    pub fn contains(&self, point: [f32; 3]) -> bool {
        for (axis, value) in point.iter().enumerate() {
            if !(self.min[axis]..=self.max[axis]).contains(value) {
                return false;
            }
        }
        true
    }
}

/// Which side of the lane a kicker sits on (left is −Z, SPEC §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The −Z side.
    Left,
    /// The +Z side.
    Right,
}

/// One course piece (SPEC §7.11). Positions are the piece's start point on the ground,
/// in m; `yaw` turns the piece about +Y, in degrees; a piece runs along its local +X.
/// Optional sizes fall back to the defaults in `course_build`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PieceSpec {
    /// A ramp curving up along +X from flat to `angle_deg` at its lip, with a sheer drop
    /// at its end.
    Ramp {
        /// Start point on the ground, in m.
        pos: [f32; 3],
        /// Heading about +Y, in degrees (0 runs along +X).
        #[serde(default)]
        yaw: f32,
        /// Length along the piece's +X, in m.
        length: f32,
        /// Width across the piece (local Z), in m.
        width: f32,
        /// Slope, in degrees.
        angle_deg: f32,
        /// The slowest clean entry that still clears what the ramp leads to (a gap), in
        /// m/s; below it the car takes the fallback (`docs/contracts/survival.md`).
        /// 0, the default, means any speed completes.
        #[serde(default, skip_serializing_if = "is_zero")]
        v_complete_min_m_s: f32,
    },
    /// A narrow ramp on one side of the lane, so only that side's wheels climb: it rolls
    /// the car.
    Kicker {
        /// Start point on the ground, in m.
        pos: [f32; 3],
        /// Heading about +Y, in degrees (0 runs along +X).
        #[serde(default)]
        yaw: f32,
        /// Length along the piece's +X, in m.
        length: f32,
        /// Width across the piece (local Z), in m.
        width: f32,
        /// Slope, in degrees.
        angle_deg: f32,
        /// Which side of the lane it sits on.
        side: Side,
    },
    /// A twisted ramp across the lane that flips the car: its low edge rises gently
    /// to `angle_deg` at the lip while the whole surface banks more and more towards
    /// `side`'s raised edge, up to `bank_deg` at a sheer lip. A car driven over it at
    /// speed leaves the lip banked hard and spinning about its nose axis, and rolls
    /// past its side in the air.
    Corkscrew {
        /// Start point on the ground, in m.
        pos: [f32; 3],
        /// Heading about +Y, in degrees (0 runs along +X).
        #[serde(default)]
        yaw: f32,
        /// Length along the piece's +X, in m.
        length: f32,
        /// Width across the piece (local Z), centred on the lane, in m.
        width: f32,
        /// Slope of the low edge at the lip, in degrees.
        angle_deg: f32,
        /// How far the surface is banked across at the lip, in degrees.
        bank_deg: f32,
        /// Which edge is raised: the car rolls away from it.
        side: Side,
    },
    /// A hole in the floor `length` long, across the whole course (runs along +X). With
    /// `pit_depth` above 0 it is a catch pit instead: a sloped floor `pit_depth` deep with
    /// an exit slope, so a car that falls short drives out (`docs/contracts/survival.md`).
    /// `entry_height` is the height of the lip it follows; the pit's floor starts there.
    Gap {
        pos: [f32; 3],
        length: f32,
        /// Depth of the catch pit below the floor, in m. 0 (the default) is a bare hole.
        #[serde(default, skip_serializing_if = "is_zero")]
        pit_depth: f32,
        /// Height of the lip before the pit, in m (only used with a pit).
        #[serde(default, skip_serializing_if = "is_zero")]
        entry_height: f32,
    },
    /// `count` parked cars nose to tail along +X, 4.5 m long, 1.9 m wide, `height` tall,
    /// 0.6 m apart. `hop_hint` tells the driver bot to Hop.
    CarRow {
        /// Start point on the ground, in m.
        pos: [f32; 3],
        /// Heading about +Y, in degrees (0 runs along +X).
        #[serde(default)]
        yaw: f32,
        /// How many items (cars, bumps) in a row.
        count: u32,
        /// Height, in m.
        height: f32,
        /// Tells the driver bot to Hop here.
        #[serde(default)]
        hop_hint: bool,
    },
    /// A U-shaped channel along +X with curved walls of `radius`.
    Halfpipe {
        /// Start point on the ground, in m.
        pos: [f32; 3],
        /// Heading about +Y, in degrees (0 runs along +X).
        #[serde(default)]
        yaw: f32,
        /// Radius of each curved wall, in m.
        radius: f32,
        /// Length along the piece's +X, in m.
        length: f32,
    },
    /// The first half of a loop along +X: the road curves up from flat through
    /// vertical to upside down at `2 × radius`, where it ends. A fast car leaves its
    /// top upside down, heading back along −X (M7 wrap-up, owner request).
    HalfLoop {
        /// Start point on the ground (the bottom of the curve), in m.
        pos: [f32; 3],
        /// Heading about +Y, in degrees (0 runs along +X).
        #[serde(default)]
        yaw: f32,
        /// Radius of the curve, in m.
        radius: f32,
        /// Width of the road, in m.
        width: f32,
        /// The slowest clean entry that completes the loop, in m/s; below it the car
        /// rolls back down the entry. 0, the default, means no minimum is published.
        #[serde(default, skip_serializing_if = "is_zero")]
        v_complete_min_m_s: f32,
    },
    /// `count` low bumps across the lane, one every few metres along +X.
    Bumps {
        /// Start point on the ground, in m.
        pos: [f32; 3],
        /// Heading about +Y, in degrees (0 runs along +X).
        #[serde(default)]
        yaw: f32,
        /// How many items (cars, bumps) in a row.
        count: u32,
        /// Height, in m.
        height: f32,
    },
    /// A plate that knocks the car once per pass: `impulse` in N·s and `angular` in N·m·s,
    /// in the piece's frame.
    KickPlate {
        /// Start point on the ground, in m.
        pos: [f32; 3],
        /// Heading about +Y, in degrees (0 runs along +X).
        #[serde(default)]
        yaw: f32,
        /// Linear impulse applied once per pass, in N·s (world frame).
        impulse: [f32; 3],
        /// Angular impulse applied once per pass, in N·m·s (world frame).
        angular: [f32; 3],
    },
    /// A wall across the lane, `height` tall, with an optional jump ramp before it.
    Wall {
        /// Start point on the ground, in m.
        pos: [f32; 3],
        /// Heading about +Y, in degrees (0 runs along +X).
        #[serde(default)]
        yaw: f32,
        /// Height, in m.
        height: f32,
        /// Adds a jump ramp in front of the wall.
        #[serde(default)]
        with_ramp: bool,
    },
    /// A marked area to land on (drawn only; it is part of the floor).
    LandingPad {
        /// Start point on the ground, in m.
        pos: [f32; 3],
        /// Heading about +Y, in degrees (0 runs along +X).
        #[serde(default)]
        yaw: f32,
    },
    /// Crossing this line makes the car's pose its respawn point.
    Checkpoint {
        /// Start point on the ground, in m.
        pos: [f32; 3],
        /// Heading about +Y, in degrees (0 runs along +X).
        #[serde(default)]
        yaw: f32,
    },
    /// Crossing this line finishes the run.
    Finish {
        /// Start point on the ground, in m.
        pos: [f32; 3],
        /// Heading about +Y, in degrees (0 runs along +X).
        #[serde(default)]
        yaw: f32,
    },
}

/// For serde: leaves a zero optional number out of the written file.
fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

/// A checked course file.
#[derive(Debug, Clone, PartialEq)]
pub struct CourseSpec {
    /// Shown in the game, e.g. "Gauntlet".
    pub name: String,
    /// Where the car begins.
    pub start: StartSpec,
    /// Leaving this box is an out-of-bounds crash.
    pub bounds: BoundsSpec,
    /// The pieces, in file order.
    pub pieces: Vec<PieceSpec>,
}

/// Why a course file was refused. Every message starts with "course.json" and names
/// what to fix.
#[derive(Debug, thiserror::Error)]
pub enum CourseError {
    /// The text is not valid JSON or does not match the schema.
    #[error("course.json: {0}")]
    Json(#[from] serde_json::Error),

    /// A top-level field has a bad value.
    #[error("course.json: `{field}` {problem}")]
    BadField { field: String, problem: String },

    /// A piece has a bad value.
    #[error("course.json: piece {index} '{kind}': {problem}")]
    BadPiece {
        index: usize,
        kind: String,
        problem: String,
    },
}

/// The file's top level as written by `CourseSpec::to_json`.
#[derive(Serialize)]
struct CourseFileOut<'a> {
    version: u32,
    name: &'a str,
    start: &'a StartSpec,
    bounds: &'a BoundsSpec,
    pieces: &'a [PieceSpec],
}

/// The top level of the file, with pieces left as raw JSON so each one can be read on
/// its own and its errors can name it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CourseFile {
    // Checked by `check_version` before this parse; listed so it is a known field.
    #[serde(rename = "version")]
    _version: u32,
    name: String,
    start: StartSpec,
    bounds: BoundsSpec,
    pieces: Vec<Value>,
}

impl CourseSpec {
    /// The course as course-file JSON (schema version 1), e.g. to hand a generated
    /// course to the renderer; `from_json_str` reads it back unchanged.
    pub fn to_json(&self) -> Result<String, CourseError> {
        let file = CourseFileOut {
            version: COURSE_SCHEMA_VERSION,
            name: &self.name,
            start: &self.start,
            bounds: &self.bounds,
            pieces: &self.pieces,
        };
        Ok(serde_json::to_string(&file)?)
    }

    /// Reads and checks the text of a course file.
    pub fn from_json_str(text: &str) -> Result<CourseSpec, CourseError> {
        // Check the version before the strict field rules, so a newer file is refused
        // for its version, not for a field this build does not know.
        let raw: Value = serde_json::from_str(text)?;
        check_version(&raw)?;
        let file: CourseFile = serde_json::from_value(raw)?;
        if file.pieces.len() > MAX_PIECES {
            let problem = format!("has {} pieces; at most {MAX_PIECES}", file.pieces.len());
            return Err(bad_field("pieces", problem));
        }
        check_bounds(&file.bounds)?;
        check_inside("start.pos", file.start.pos, &file.bounds)?;
        check_finite("start.yaw", &[file.start.yaw])?;
        let mut pieces = Vec::with_capacity(file.pieces.len());
        for (index, raw) in file.pieces.into_iter().enumerate() {
            let piece = read_piece(index, raw)?;
            check_piece(index, &piece, &file.bounds)?;
            pieces.push(piece);
        }
        Ok(CourseSpec {
            name: file.name,
            start: file.start,
            bounds: file.bounds,
            pieces,
        })
    }
}

/// Refuses a file whose `version` is a number other than this build's. A missing or
/// non-number version is left for the field rules to report.
fn check_version(raw: &Value) -> Result<(), CourseError> {
    let Some(version) = raw.get("version").and_then(Value::as_u64) else {
        return Ok(());
    };
    if version != u64::from(COURSE_SCHEMA_VERSION) {
        let problem = format!("must be {COURSE_SCHEMA_VERSION}, not {version}");
        return Err(bad_field("version", problem));
    }
    Ok(())
}

impl PieceSpec {
    /// The piece's `type` name, as in the file.
    pub fn kind(&self) -> &'static str {
        match self {
            PieceSpec::Ramp { .. } => "ramp",
            PieceSpec::Kicker { .. } => "kicker",
            PieceSpec::Corkscrew { .. } => "corkscrew",
            PieceSpec::Gap { .. } => "gap",
            PieceSpec::CarRow { .. } => "car_row",
            PieceSpec::Halfpipe { .. } => "halfpipe",
            PieceSpec::HalfLoop { .. } => "half_loop",
            PieceSpec::Bumps { .. } => "bumps",
            PieceSpec::KickPlate { .. } => "kick_plate",
            PieceSpec::Wall { .. } => "wall",
            PieceSpec::LandingPad { .. } => "landing_pad",
            PieceSpec::Checkpoint { .. } => "checkpoint",
            PieceSpec::Finish { .. } => "finish",
        }
    }

    /// The piece's start point on the ground, in m.
    pub fn position(&self) -> [f32; 3] {
        match self {
            PieceSpec::Ramp { pos, .. }
            | PieceSpec::Kicker { pos, .. }
            | PieceSpec::Corkscrew { pos, .. }
            | PieceSpec::Gap { pos, .. }
            | PieceSpec::CarRow { pos, .. }
            | PieceSpec::Halfpipe { pos, .. }
            | PieceSpec::HalfLoop { pos, .. }
            | PieceSpec::Bumps { pos, .. }
            | PieceSpec::KickPlate { pos, .. }
            | PieceSpec::Wall { pos, .. }
            | PieceSpec::LandingPad { pos, .. }
            | PieceSpec::Checkpoint { pos, .. }
            | PieceSpec::Finish { pos, .. } => *pos,
        }
    }
}

/// Reads piece `index` from its raw JSON; errors name the piece.
fn read_piece(index: usize, raw: Value) -> Result<PieceSpec, CourseError> {
    let kind = raw
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("(no type)")
        .to_string();
    serde_json::from_value(raw).map_err(|error| CourseError::BadPiece {
        index,
        kind,
        problem: error.to_string(),
    })
}

/// Checks one piece's numbers: finite, sizes above zero and not absurd, counts in range,
/// angles that make a usable slope, and its start inside the course bounds.
fn check_piece(index: usize, piece: &PieceSpec, bounds: &BoundsSpec) -> Result<(), CourseError> {
    let fail = |problem: String| CourseError::BadPiece {
        index,
        kind: piece.kind().to_string(),
        problem,
    };
    let numbers = piece_numbers(piece);
    if numbers.iter().any(|value| !value.is_finite()) {
        return Err(fail("every number must be finite".to_string()));
    }
    for (name, size) in piece_sizes(piece) {
        if !(MIN_PIECE_SIZE_M..=MAX_PIECE_SIZE_M).contains(&size) {
            return Err(fail(format!(
                "`{name}` must be {MIN_PIECE_SIZE_M} to {MAX_PIECE_SIZE_M} m, not {size}"
            )));
        }
    }
    if let Some(count) = piece_count(piece)
        && !(1..=MAX_COUNT).contains(&count)
    {
        return Err(fail(format!(
            "`count` must be 1 to {MAX_COUNT}, not {count}"
        )));
    }
    if let PieceSpec::Ramp { angle_deg, .. }
    | PieceSpec::Kicker { angle_deg, .. }
    | PieceSpec::Corkscrew { angle_deg, .. } = piece
        && !(MIN_RAMP_ANGLE_DEG..MAX_RAMP_ANGLE_DEG).contains(angle_deg)
    {
        return Err(fail(format!(
            "`angle_deg` must be at least {MIN_RAMP_ANGLE_DEG} and below {MAX_RAMP_ANGLE_DEG}, not {angle_deg}"
        )));
    }
    if let PieceSpec::Corkscrew { bank_deg, .. } = piece
        && !(MIN_BANK_DEG..MAX_BANK_DEG).contains(bank_deg)
    {
        return Err(fail(format!(
            "`bank_deg` must be at least {MIN_BANK_DEG} and below {MAX_BANK_DEG}, not {bank_deg}"
        )));
    }
    check_fallback_fields(piece).map_err(fail)?;
    if !bounds.contains(piece.position()) {
        return Err(fail(format!(
            "`pos` {:?} is outside the course bounds",
            piece.position()
        )));
    }
    Ok(())
}

/// Checks the optional survival fields: a minimum completion speed and a catch pit's size.
fn check_fallback_fields(piece: &PieceSpec) -> Result<(), String> {
    match piece {
        PieceSpec::Ramp {
            v_complete_min_m_s, ..
        }
        | PieceSpec::HalfLoop {
            v_complete_min_m_s, ..
        } if !(0.0..=MAX_V_COMPLETE_MIN_M_S).contains(v_complete_min_m_s) => Err(format!(
            "`v_complete_min_m_s` must be 0 to {MAX_V_COMPLETE_MIN_M_S}, not {v_complete_min_m_s}"
        )),
        PieceSpec::Gap {
            pit_depth,
            entry_height,
            ..
        } if !(0.0..=MAX_PIT_DEPTH_M).contains(pit_depth)
            || !(0.0..=MAX_PIECE_SIZE_M).contains(entry_height) =>
        {
            Err(format!(
                "`pit_depth` must be 0 to {MAX_PIT_DEPTH_M} m and `entry_height` 0 to {MAX_PIECE_SIZE_M} m, not {pit_depth} and {entry_height}"
            ))
        }
        _ => Ok(()),
    }
}

/// Every number in a piece, for the finite check.
fn piece_numbers(piece: &PieceSpec) -> Vec<f32> {
    let mut numbers = piece.position().to_vec();
    numbers.extend(piece_sizes(piece).into_iter().map(|(_, size)| size));
    match piece {
        PieceSpec::Ramp {
            yaw,
            angle_deg,
            v_complete_min_m_s,
            ..
        } => numbers.extend([*yaw, *angle_deg, *v_complete_min_m_s]),
        PieceSpec::Kicker { yaw, angle_deg, .. } => numbers.extend([*yaw, *angle_deg]),
        PieceSpec::HalfLoop {
            yaw,
            v_complete_min_m_s,
            ..
        } => numbers.extend([*yaw, *v_complete_min_m_s]),
        PieceSpec::Corkscrew {
            yaw,
            angle_deg,
            bank_deg,
            ..
        } => numbers.extend([*yaw, *angle_deg, *bank_deg]),
        PieceSpec::KickPlate {
            yaw,
            impulse,
            angular,
            ..
        } => {
            numbers.push(*yaw);
            numbers.extend(impulse);
            numbers.extend(angular);
        }
        PieceSpec::CarRow { yaw, .. }
        | PieceSpec::Halfpipe { yaw, .. }
        | PieceSpec::Bumps { yaw, .. }
        | PieceSpec::Wall { yaw, .. }
        | PieceSpec::LandingPad { yaw, .. }
        | PieceSpec::Checkpoint { yaw, .. }
        | PieceSpec::Finish { yaw, .. } => numbers.push(*yaw),
        PieceSpec::Gap {
            pit_depth,
            entry_height,
            ..
        } => numbers.extend([*pit_depth, *entry_height]),
    }
    numbers
}

/// The sizes a piece declares, by field name; each must be above zero.
fn piece_sizes(piece: &PieceSpec) -> Vec<(&'static str, f32)> {
    match piece {
        PieceSpec::Ramp { length, width, .. }
        | PieceSpec::Kicker { length, width, .. }
        | PieceSpec::Corkscrew { length, width, .. } => {
            vec![("length", *length), ("width", *width)]
        }
        PieceSpec::Gap { length, .. } => vec![("length", *length)],
        PieceSpec::CarRow { height, .. }
        | PieceSpec::Bumps { height, .. }
        | PieceSpec::Wall { height, .. } => {
            vec![("height", *height)]
        }
        PieceSpec::Halfpipe { radius, length, .. } => {
            vec![("radius", *radius), ("length", *length)]
        }
        PieceSpec::HalfLoop { radius, width, .. } => {
            vec![("radius", *radius), ("width", *width)]
        }
        PieceSpec::KickPlate { .. }
        | PieceSpec::LandingPad { .. }
        | PieceSpec::Checkpoint { .. }
        | PieceSpec::Finish { .. } => Vec::new(),
    }
}

/// How many items a piece repeats, if it repeats any.
fn piece_count(piece: &PieceSpec) -> Option<u32> {
    match piece {
        PieceSpec::CarRow { count, .. } | PieceSpec::Bumps { count, .. } => Some(*count),
        _ => None,
    }
}

/// The bounds must be finite with min below max on every axis.
fn check_bounds(bounds: &BoundsSpec) -> Result<(), CourseError> {
    check_finite("bounds", &[bounds.min, bounds.max].concat())?;
    for value in bounds.min.iter().chain(bounds.max.iter()) {
        if value.abs() > MAX_BOUNDS_M {
            return Err(bad_field(
                "bounds",
                format!("corners must lie within ±{MAX_BOUNDS_M} m, not {value}"),
            ));
        }
    }
    for axis in 0..3 {
        if bounds.min[axis] >= bounds.max[axis] {
            return Err(bad_field(
                "bounds",
                "min must be below max on every axis".to_string(),
            ));
        }
    }
    Ok(())
}

/// `point` must lie inside the bounds.
fn check_inside(field: &str, point: [f32; 3], bounds: &BoundsSpec) -> Result<(), CourseError> {
    check_finite(field, &point)?;
    if !bounds.contains(point) {
        return Err(bad_field(field, "is outside the course bounds".to_string()));
    }
    Ok(())
}

fn check_finite(field: &str, values: &[f32]) -> Result<(), CourseError> {
    if values.iter().all(|value| value.is_finite()) {
        return Ok(());
    }
    Err(bad_field(field, "must be finite numbers".to_string()))
}

fn bad_field(field: &str, problem: String) -> CourseError {
    CourseError::BadField {
        field: field.to_string(),
        problem,
    }
}
