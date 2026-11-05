//! Approach is separate from outcome (docs/contracts/traversal.md): a record kept while the
//! car goes over one course piece, answering "did the car come in cleanly?" apart from
//! "what happened next?". It never makes the car immune: crash classification always uses
//! the real contact geometry. It feeds the HUD hint, test assertions and the survival
//! metric (docs/contracts/survival.md), and raises `TooSlowFallback` when a clean car
//! enters a piece below that piece's `v_complete_min_m_s`.
//!
//! This file also holds `ApproachConfig`, the reader of `configs/approach.toml`: the
//! clean-entry limits and the scrape rule's numbers (docs/contracts/crash-scrape.md).

use serde::{Deserialize, Serialize};

use crate::course::{CourseSpec, PieceSpec};
use crate::course_build::{LANE_WIDTH_M, wall_ramp_total_length_m};

/// The repo's `configs/approach.toml`, built in like the other configs.
pub use crate::provenance::DEFAULT_APPROACH_TOML;

/// `configs/approach.toml`: when an entry is clean and when a body contact is a Scrape.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApproachConfig {
    /// Largest heading error against the piece's forward axis, in degrees.
    pub max_heading_err_deg: f32,
    /// Largest pitch relative to the piece surface, in degrees.
    pub max_pitch_deg: f32,
    /// Largest roll, in degrees.
    pub max_roll_deg: f32,
    /// The car's centre must stay inside the piece width minus this margin, in m, on top
    /// of the car's half width.
    pub lateral_margin_m: f32,
    /// How far the car's wheels and body reach either side of its centre line, in m. A
    /// narrow piece (a 4 m corkscrew) is only clean if the whole car fits on it.
    pub car_half_width_m: f32,
    /// Fewest wheels on the ground at entry, for ground-entry pieces.
    pub min_wheels_down: u32,
    /// The record times out after piece length / entry speed plus this, in s.
    pub timeout_extra_s: f32,
    /// Scrape rule 1: the contact is this far inside the body's lower face, in m.
    pub scrape_underbody_margin_m: f32,
    /// Scrape rule 2: the contact normal is within this angle of body up, in degrees.
    pub scrape_normal_deg: f32,
    /// Scrape rule 3: the normal impact speed is at most this, in m/s.
    pub scrape_max_normal_m_s: f32,
    /// Scrape rule 4: the tilt is under this, in degrees.
    pub scrape_max_tilt_deg: f32,
    /// One long scrape reports one `Scrape` event; a new one is reported after the
    /// contact has been gone this long, in s.
    pub scrape_event_gap_s: f32,
}

/// Why approach.toml could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ApproachConfigError {
    #[error("approach.toml: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("approach.toml: `{field}` must be a finite number above zero, not {value}")]
    NotPositive { field: &'static str, value: f32 },
}

impl ApproachConfig {
    /// Parses and checks the text of an approach.toml.
    pub fn from_toml_str(text: &str) -> Result<ApproachConfig, ApproachConfigError> {
        let config: ApproachConfig = toml::from_str(text)?;
        config.check()?;
        Ok(config)
    }

    /// The built-in approach.toml. It is tested to parse, so this cannot fail.
    pub fn built_in() -> ApproachConfig {
        match ApproachConfig::from_toml_str(DEFAULT_APPROACH_TOML) {
            Ok(config) => config,
            Err(error) => panic!("the built-in approach.toml is broken: {error}"),
        }
    }

    fn check(&self) -> Result<(), ApproachConfigError> {
        let fields = [
            ("max_heading_err_deg", self.max_heading_err_deg),
            ("max_pitch_deg", self.max_pitch_deg),
            ("max_roll_deg", self.max_roll_deg),
            ("lateral_margin_m", self.lateral_margin_m),
            ("car_half_width_m", self.car_half_width_m),
            ("timeout_extra_s", self.timeout_extra_s),
            ("scrape_underbody_margin_m", self.scrape_underbody_margin_m),
            ("scrape_normal_deg", self.scrape_normal_deg),
            ("scrape_max_normal_m_s", self.scrape_max_normal_m_s),
            ("scrape_max_tilt_deg", self.scrape_max_tilt_deg),
            ("scrape_event_gap_s", self.scrape_event_gap_s),
        ];
        for (field, value) in fields {
            if !value.is_finite() || value <= 0.0 {
                return Err(ApproachConfigError::NotPositive { field, value });
            }
        }
        Ok(())
    }
}

/// The first check an entry failed (the contract's `reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApproachFailure {
    /// The heading is too far from the piece's forward axis.
    Heading,
    /// The car is too far from the piece's centre line.
    Lateral,
    /// The pitch is too far from the piece surface.
    Pitch,
    /// The car is rolled too far.
    Roll,
    /// Too few wheels on the ground.
    WheelsDown,
}

/// How a car came onto a piece, judged in the piece's local frame (traversal.md).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Approach {
    /// True if every limit held.
    pub clean: bool,
    /// Angle between the car's heading and the piece's forward axis, in rad (absolute).
    pub heading_err_rad: f32,
    /// Distance from the piece's centre line, in m (absolute).
    pub lateral_offset_m: f32,
    /// Pitch of the nose above the piece surface at its foot (flat), in rad (absolute).
    pub pitch_rad: f32,
    /// Roll about the nose axis, in rad (absolute).
    pub roll_rad: f32,
    /// How many wheels touched the ground.
    pub wheels_down: u32,
    /// The first limit that failed, if any.
    pub reason: Option<ApproachFailure>,
}

/// Where the car is in a piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraversalPhase {
    /// Just crossed the entry line.
    Entering,
    /// On the piece's surface.
    Traversing,
    /// Wheels off the ground over the piece.
    Airborne,
    /// Past the piece, or the record timed out.
    Exited,
}

/// The record of one pass over one piece (traversal.md).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PieceTraversal {
    /// Index of the piece in the course file.
    pub piece_id: usize,
    /// Physics step on which the car crossed the entry line.
    pub entry_tick: u64,
    /// Speed along the piece's forward axis at entry, in m/s.
    pub entry_speed_m_s: f32,
    /// How the car came in.
    pub approach: Approach,
    /// Where the car is now.
    pub phase: TraversalPhase,
    /// True if the entry was below the piece's `v_complete_min_m_s`.
    pub too_slow: bool,
}

/// A piece the tracker watches: its frame and size, from the course file.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PieceFootprint {
    /// Index in the course file.
    pub piece_id: usize,
    /// Start point on the ground (x, z), in m.
    pub origin_xz_m: [f32; 2],
    /// Heading about +Y, in rad.
    pub yaw_rad: f32,
    /// Length along the piece's forward axis, in m.
    pub length_m: f32,
    /// Width across the piece, in m.
    pub width_m: f32,
    /// The slowest clean entry that completes the piece, in m/s (0: any speed).
    pub v_complete_min_m_s: f32,
}

impl PieceFootprint {
    /// The piece's forward axis (local +X) on the ground, as (x, z).
    fn forward_xz(&self) -> [f32; 2] {
        [libm::cosf(self.yaw_rad), -libm::sinf(self.yaw_rad)]
    }

    /// The car's position in the piece frame: (along, across), in m.
    pub fn local_xz_m(&self, position_m: [f32; 3]) -> [f32; 2] {
        let [fx, fz] = self.forward_xz();
        let dx = position_m[0] - self.origin_xz_m[0];
        let dz = position_m[2] - self.origin_xz_m[1];
        // Local +Z is the forward axis turned a quarter: (sin yaw, cos yaw) = (-fz, fx).
        [dx * fx + dz * fz, -dx * fz + dz * fx]
    }
}

/// The footprints of the course's pieces that have a clean entry: ramps, kickers,
/// corkscrews, halfpipes, half loops and walls with a ramp.
pub fn footprints_of(course: &CourseSpec) -> Vec<PieceFootprint> {
    let mut footprints = Vec::new();
    for (piece_id, piece) in course.pieces.iter().enumerate() {
        if let Some(footprint) = footprint_of(piece_id, piece) {
            footprints.push(footprint);
        }
    }
    footprints
}

fn footprint_of(piece_id: usize, piece: &PieceSpec) -> Option<PieceFootprint> {
    let make = |pos: &[f32; 3], yaw: f32, length_m: f32, width_m: f32, v_min: f32| PieceFootprint {
        piece_id,
        origin_xz_m: [pos[0], pos[2]],
        yaw_rad: yaw.to_radians(),
        length_m,
        width_m,
        v_complete_min_m_s: v_min,
    };
    match piece {
        PieceSpec::Ramp {
            pos,
            yaw,
            length,
            width,
            v_complete_min_m_s,
            ..
        } => Some(make(pos, *yaw, *length, *width, *v_complete_min_m_s)),
        PieceSpec::Kicker {
            pos, yaw, length, ..
        } => Some(make(pos, *yaw, *length, LANE_WIDTH_M, 0.0)),
        PieceSpec::Corkscrew {
            pos,
            yaw,
            length,
            width,
            ..
        } => Some(make(pos, *yaw, *length, *width, 0.0)),
        PieceSpec::Halfpipe {
            pos, yaw, length, ..
        } => Some(make(pos, *yaw, *length, LANE_WIDTH_M, 0.0)),
        PieceSpec::HalfLoop {
            pos,
            yaw,
            radius,
            width,
            v_complete_min_m_s,
        } => Some(make(pos, *yaw, 2.0 * radius, *width, *v_complete_min_m_s)),
        PieceSpec::Wall {
            pos,
            yaw,
            height,
            with_ramp: true,
        } => Some(make(
            pos,
            *yaw,
            wall_ramp_total_length_m(*height),
            LANE_WIDTH_M,
            0.0,
        )),
        _ => None,
    }
}

/// What the car looks like to the tracker, each step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarSample {
    /// Centre of mass, in m.
    pub position_m: [f32; 3],
    /// Velocity of the centre of mass, in m/s.
    pub velocity_m_s: [f32; 3],
    /// The car's nose direction in the world frame (unit).
    pub nose_world: [f32; 3],
    /// The car's up direction in the world frame (unit).
    pub up_world: [f32; 3],
    /// How many wheels touch the ground.
    pub wheels_down: u32,
}

/// Judges an entry at `sample` onto `footprint` against `limits` (traversal.md "When an
/// entry is clean"). `airborne_exempt` skips the wheel check (flip plates).
pub fn judge_approach(
    limits: &ApproachConfig,
    footprint: &PieceFootprint,
    sample: &CarSample,
    airborne_exempt: bool,
) -> Approach {
    let [fx, fz] = footprint.forward_xz();
    let nose_xz_len = libm::hypotf(sample.nose_world[0], sample.nose_world[2]).max(1e-6);
    let along = (sample.nose_world[0] * fx + sample.nose_world[2] * fz) / nose_xz_len;
    let cross = (-sample.nose_world[0] * fz + sample.nose_world[2] * fx) / nose_xz_len;
    let heading_err_rad = libm::atan2f(cross.abs(), along);
    let lateral_offset_m = footprint.local_xz_m(sample.position_m)[1].abs();
    let pitch_rad = libm::asinf(sample.nose_world[1].clamp(-1.0, 1.0)).abs();
    let roll_rad = roll_about_nose_rad(sample.nose_world, sample.up_world);
    let approach = Approach {
        clean: true,
        heading_err_rad,
        lateral_offset_m,
        pitch_rad,
        roll_rad,
        wheels_down: sample.wheels_down,
        reason: None,
    };
    let reason = first_failure(limits, footprint, &approach, airborne_exempt);
    Approach {
        clean: reason.is_none(),
        reason,
        ..approach
    }
}

/// The first limit `approach` breaks, in the contract's table order.
fn first_failure(
    limits: &ApproachConfig,
    footprint: &PieceFootprint,
    approach: &Approach,
    airborne_exempt: bool,
) -> Option<ApproachFailure> {
    if approach.heading_err_rad > limits.max_heading_err_deg.to_radians() {
        return Some(ApproachFailure::Heading);
    }
    let half_width_m = footprint.width_m / 2.0 - limits.lateral_margin_m - limits.car_half_width_m;
    if approach.lateral_offset_m > half_width_m {
        return Some(ApproachFailure::Lateral);
    }
    if approach.pitch_rad > limits.max_pitch_deg.to_radians() {
        return Some(ApproachFailure::Pitch);
    }
    if approach.roll_rad > limits.max_roll_deg.to_radians() {
        return Some(ApproachFailure::Roll);
    }
    if !airborne_exempt && approach.wheels_down < limits.min_wheels_down {
        return Some(ApproachFailure::WheelsDown);
    }
    None
}

/// The angle between the car's up axis and the world's up axis taken in the plane across
/// the nose, in rad (absolute): roll without the pitch.
fn roll_about_nose_rad(nose: [f32; 3], up: [f32; 3]) -> f32 {
    // World up with its nose component removed, then compared with the car's up.
    let world_up = [0.0, 1.0, 0.0];
    let nose_part = nose[1];
    let flat_up = [
        world_up[0] - nose_part * nose[0],
        world_up[1] - nose_part * nose[1],
        world_up[2] - nose_part * nose[2],
    ];
    let flat_len = libm::sqrtf(flat_up.iter().map(|v| v * v).sum::<f32>()).max(1e-6);
    let cos = (flat_up[0] * up[0] + flat_up[1] * up[1] + flat_up[2] * up[2]) / flat_len;
    libm::acosf(cos.clamp(-1.0, 1.0))
}

/// Watches the car against a course's pieces and keeps the traversal records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraversalTracker {
    limits: ApproachConfig,
    footprints: Vec<PieceFootprint>,
    /// The last car position's local x per footprint, to spot crossing the entry line.
    last_along_m: Vec<Option<f32>>,
    /// The record in progress, if any.
    active: Option<PieceTraversal>,
    /// Records that have ended, oldest first.
    finished: Vec<PieceTraversal>,
}

/// Something the tracker noticed this step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TraversalNote {
    /// A car entered a piece below its `v_complete_min_m_s` and takes the fallback.
    TooSlow {
        /// Index of the piece in the course file.
        piece_id: usize,
        /// The entry speed, in m/s.
        entry_speed_m_s: f32,
    },
}

/// How far before a piece's entry line the approach hint starts, in m.
const APPROACH_LOOKAHEAD_M: f32 = 80.0;
/// How far beside a piece's edge the car may be and still count as approaching it, in m.
const APPROACH_SIDE_MARGIN_M: f32 = 2.0;
/// The widest angle between the nose and the piece's forward axis for the hint, in rad
/// (60 degrees).
const APPROACH_HEADING_MAX_RAD: f32 = core::f32::consts::FRAC_PI_3;

/// Most finished records kept.
const MAX_FINISHED: usize = 64;

impl TraversalTracker {
    /// A tracker for `course`'s pieces.
    pub fn for_course(course: &CourseSpec, limits: ApproachConfig) -> TraversalTracker {
        let footprints = footprints_of(course);
        TraversalTracker {
            limits,
            last_along_m: vec![None; footprints.len()],
            footprints,
            active: None,
            finished: Vec::new(),
        }
    }

    /// A tracker that watches nothing (flat ground).
    pub fn empty() -> TraversalTracker {
        TraversalTracker {
            limits: ApproachConfig::built_in(),
            footprints: Vec::new(),
            last_along_m: Vec::new(),
            active: None,
            finished: Vec::new(),
        }
    }

    /// The record in progress, if the car is on a piece.
    pub fn active(&self) -> Option<&PieceTraversal> {
        self.active.as_ref()
    }

    /// Ended records, oldest first.
    pub fn finished(&self) -> &[PieceTraversal] {
        &self.finished
    }

    /// The limits in use.
    pub fn limits(&self) -> &ApproachConfig {
        &self.limits
    }

    /// The approach hint (`HudField` 26): when a piece with a `v_complete_min_m_s` is
    /// ahead and the car is slower than it, that piece's `v_complete_min_m_s` in m/s, else
    /// 0. A piece is "ahead" when the car is before its entry line (no further than
    /// `APPROACH_LOOKAHEAD_M`), within its width plus `APPROACH_SIDE_MARGIN_M`, and the
    /// nose points within `APPROACH_HEADING_MAX_RAD` of the piece's forward axis. Reads
    /// only; it keeps no state, so it cannot change a run. The nearest such piece wins.
    pub fn approach_hint_m_s(&self, sample: &CarSample) -> f32 {
        if self.active.is_some() {
            return 0.0;
        }
        let mut nearest: Option<(f32, f32)> = None;
        for footprint in &self.footprints {
            if footprint.v_complete_min_m_s <= 0.0 {
                continue;
            }
            let [along_m, across_m] = footprint.local_xz_m(sample.position_m);
            let distance_m = -along_m;
            if !(0.0..=APPROACH_LOOKAHEAD_M).contains(&distance_m)
                || across_m.abs() > 0.5 * footprint.width_m + APPROACH_SIDE_MARGIN_M
            {
                continue;
            }
            let [fx, fz] = footprint.forward_xz();
            let nose_xz_len = libm::hypotf(sample.nose_world[0], sample.nose_world[2]).max(1e-6);
            let aligned = (sample.nose_world[0] * fx + sample.nose_world[2] * fz) / nose_xz_len;
            if libm::acosf(aligned.clamp(-1.0, 1.0)) > APPROACH_HEADING_MAX_RAD {
                continue;
            }
            let speed_m_s = sample.velocity_m_s[0] * fx + sample.velocity_m_s[2] * fz;
            if speed_m_s < footprint.v_complete_min_m_s
                && nearest.is_none_or(|(best_m, _)| distance_m < best_m)
            {
                nearest = Some((distance_m, footprint.v_complete_min_m_s));
            }
        }
        nearest.map_or(0.0, |(_, v_min_m_s)| v_min_m_s)
    }

    /// Forgets all records and positions (the car was moved).
    pub fn reset(&mut self) {
        self.active = None;
        self.finished.clear();
        self.last_along_m.fill(None);
    }

    /// Folds in one physics step. `tick` counts steps; `dt_s` is the step length.
    pub fn update(&mut self, tick: u64, dt_s: f32, sample: &CarSample) -> Vec<TraversalNote> {
        let mut notes = Vec::new();
        self.advance_active(tick, dt_s, sample);
        for index in 0..self.footprints.len() {
            let footprint = self.footprints[index];
            let along_m = footprint.local_xz_m(sample.position_m)[0];
            let before = self.last_along_m[index].replace(along_m);
            let crossed = matches!(before, Some(previous) if previous < 0.0) && along_m >= 0.0;
            if crossed && along_m < footprint.length_m && self.active.is_none() {
                notes.extend(self.begin(tick, &footprint, sample));
            }
        }
        notes
    }

    /// Starts a record for `footprint` at this step.
    fn begin(
        &mut self,
        tick: u64,
        footprint: &PieceFootprint,
        sample: &CarSample,
    ) -> Option<TraversalNote> {
        let [fx, fz] = footprint.forward_xz();
        let entry_speed_m_s = sample.velocity_m_s[0] * fx + sample.velocity_m_s[2] * fz;
        let approach = judge_approach(&self.limits, footprint, sample, false);
        let too_slow = entry_speed_m_s < footprint.v_complete_min_m_s;
        self.active = Some(PieceTraversal {
            piece_id: footprint.piece_id,
            entry_tick: tick,
            entry_speed_m_s,
            approach,
            phase: TraversalPhase::Entering,
            too_slow,
        });
        too_slow.then_some(TraversalNote::TooSlow {
            piece_id: footprint.piece_id,
            entry_speed_m_s,
        })
    }

    /// Moves the active record through its phases and ends it past the piece or on timeout.
    fn advance_active(&mut self, tick: u64, dt_s: f32, sample: &CarSample) {
        let Some(mut record) = self.active else {
            return;
        };
        let Some(footprint) = self
            .footprints
            .iter()
            .find(|footprint| footprint.piece_id == record.piece_id)
        else {
            self.active = None;
            return;
        };
        let along_m = footprint.local_xz_m(sample.position_m)[0];
        let steps = (tick - record.entry_tick) as f32;
        let speed_m_s = record.entry_speed_m_s.max(1.0);
        let timeout_s = footprint.length_m / speed_m_s + self.limits.timeout_extra_s;
        let timed_out = steps * dt_s > timeout_s;
        let past_piece = along_m > footprint.length_m || along_m < -1.0;
        if past_piece || timed_out {
            record.phase = TraversalPhase::Exited;
            self.finish(record);
            return;
        }
        record.phase = if sample.wheels_down == 0 {
            TraversalPhase::Airborne
        } else {
            TraversalPhase::Traversing
        };
        self.active = Some(record);
    }

    fn finish(&mut self, record: PieceTraversal) {
        self.active = None;
        self.finished.push(record);
        if self.finished.len() > MAX_FINISHED {
            self.finished.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> ApproachConfig {
        ApproachConfig::built_in()
    }

    fn footprint() -> PieceFootprint {
        PieceFootprint {
            piece_id: 3,
            origin_xz_m: [100.0, 0.0],
            yaw_rad: 0.0,
            length_m: 20.0,
            width_m: 8.0,
            v_complete_min_m_s: 12.0,
        }
    }

    fn level_car_at(x_m: f32, z_m: f32) -> CarSample {
        CarSample {
            position_m: [x_m, 0.6, z_m],
            velocity_m_s: [30.0, 0.0, 0.0],
            nose_world: [1.0, 0.0, 0.0],
            up_world: [0.0, 1.0, 0.0],
            wheels_down: 4,
        }
    }

    #[test]
    fn the_built_in_approach_config_parses() {
        let config = ApproachConfig::built_in();

        assert_eq!(config.min_wheels_down, 3);
        assert!(config.scrape_max_normal_m_s < 6.0);
    }

    #[test]
    fn an_unknown_approach_field_is_refused() {
        let text = format!("{DEFAULT_APPROACH_TOML}\nbogus = 1\n");

        assert!(ApproachConfig::from_toml_str(&text).is_err());
    }

    #[test]
    fn a_level_centred_entry_is_clean() {
        let approach = judge_approach(&limits(), &footprint(), &level_car_at(100.0, 0.0), false);

        assert!(approach.clean);
        assert_eq!(approach.reason, None);
    }

    #[test]
    fn heading_flips_at_the_limit() {
        let limit_deg = limits().max_heading_err_deg;
        let yawed = |deg: f32| {
            let mut car = level_car_at(100.0, 0.0);
            car.nose_world = [
                libm::cosf(deg.to_radians()),
                0.0,
                libm::sinf(deg.to_radians()),
            ];
            car
        };

        let inside = judge_approach(&limits(), &footprint(), &yawed(limit_deg - 0.5), false);
        let outside = judge_approach(&limits(), &footprint(), &yawed(limit_deg + 0.5), false);

        assert!(inside.clean);
        assert_eq!(outside.reason, Some(ApproachFailure::Heading));
    }

    #[test]
    fn lateral_offset_flips_at_half_the_width_minus_the_margin_and_the_car_half_width() {
        let limit_m =
            footprint().width_m / 2.0 - limits().lateral_margin_m - limits().car_half_width_m;

        let inside = judge_approach(
            &limits(),
            &footprint(),
            &level_car_at(100.0, limit_m - 0.05),
            false,
        );
        let outside = judge_approach(
            &limits(),
            &footprint(),
            &level_car_at(100.0, limit_m + 0.05),
            false,
        );
        let other_side = judge_approach(
            &limits(),
            &footprint(),
            &level_car_at(100.0, -limit_m - 0.05),
            false,
        );

        assert!(inside.clean);
        assert_eq!(outside.reason, Some(ApproachFailure::Lateral));
        assert_eq!(other_side.reason, Some(ApproachFailure::Lateral));
    }

    #[test]
    fn pitch_flips_at_the_limit() {
        let limit_deg = limits().max_pitch_deg;
        let pitched = |deg: f32| {
            let mut car = level_car_at(100.0, 0.0);
            let rad = deg.to_radians();
            car.nose_world = [libm::cosf(rad), libm::sinf(rad), 0.0];
            car.up_world = [-libm::sinf(rad), libm::cosf(rad), 0.0];
            car
        };

        let inside = judge_approach(&limits(), &footprint(), &pitched(limit_deg - 0.5), false);
        let outside = judge_approach(&limits(), &footprint(), &pitched(limit_deg + 0.5), false);

        assert!(inside.clean);
        assert_eq!(outside.reason, Some(ApproachFailure::Pitch));
    }

    #[test]
    fn roll_flips_at_the_limit() {
        let limit_deg = limits().max_roll_deg;
        let rolled = |deg: f32| {
            let mut car = level_car_at(100.0, 0.0);
            let rad = deg.to_radians();
            car.up_world = [0.0, libm::cosf(rad), libm::sinf(rad)];
            car
        };

        let inside = judge_approach(&limits(), &footprint(), &rolled(limit_deg - 0.5), false);
        let outside = judge_approach(&limits(), &footprint(), &rolled(limit_deg + 0.5), false);

        assert!(inside.clean);
        assert_eq!(outside.reason, Some(ApproachFailure::Roll));
    }

    #[test]
    fn wheels_down_matter_unless_the_entry_is_exempt() {
        let mut car = level_car_at(100.0, 0.0);
        car.wheels_down = 2;

        let strict = judge_approach(&limits(), &footprint(), &car, false);
        let exempt = judge_approach(&limits(), &footprint(), &car, true);

        assert_eq!(strict.reason, Some(ApproachFailure::WheelsDown));
        assert!(exempt.clean);
    }

    #[test]
    fn the_piece_frame_follows_the_yaw() {
        let mut turned = footprint();
        turned.yaw_rad = 90.0_f32.to_radians();

        // Yaw 90° runs the piece along world -Z.
        let [along, across] = turned.local_xz_m([100.0, 0.6, -10.0]);

        assert!((along - 10.0).abs() < 1e-3);
        assert!(across.abs() < 1e-3);
    }

    fn course_with_ramp() -> CourseSpec {
        CourseSpec::from_json_str(
            r#"{"version": 1, "name": "t", "start": {"pos": [0, 0.7, 0], "yaw": 0},
            "bounds": {"min": [-50, -20, -50], "max": [300, 60, 50]},
            "pieces": [{"type": "ramp", "pos": [100, 0, 0], "length": 20, "width": 8,
                        "angle_deg": 10, "v_complete_min_m_s": 12}]}"#,
        )
        .unwrap_or_else(|error| panic!("test course: {error}"))
    }

    #[test]
    fn a_slow_clean_entry_raises_too_slow_and_a_fast_one_does_not() {
        for (speed_m_s, expect_too_slow) in [(8.0, true), (20.0, false)] {
            let mut tracker = TraversalTracker::for_course(&course_with_ramp(), limits());
            let mut notes = Vec::new();
            for tick in 0..4 {
                let mut car = level_car_at(99.0 + tick as f32, 0.0);
                car.velocity_m_s = [speed_m_s, 0.0, 0.0];
                notes.extend(tracker.update(tick, 1.0 / 120.0, &car));
            }

            assert_eq!(!notes.is_empty(), expect_too_slow, "speed {speed_m_s}");
            let record = tracker.active().copied();
            assert!(record.is_some_and(|r| r.approach.clean && r.too_slow == expect_too_slow));
        }
    }

    fn hint_at(x_m: f32, z_m: f32, speed_m_s: f32) -> f32 {
        let tracker = TraversalTracker::for_course(&course_with_ramp(), limits());
        let mut car = level_car_at(x_m, z_m);
        car.velocity_m_s = [speed_m_s, 0.0, 0.0];
        tracker.approach_hint_m_s(&car)
    }

    #[test]
    fn the_hint_names_the_needed_speed_only_when_the_car_is_too_slow() {
        assert_eq!(hint_at(60.0, 0.0, 8.0), 12.0);
        assert_eq!(hint_at(60.0, 0.0, 12.0), 0.0);
        assert_eq!(hint_at(60.0, 0.0, 30.0), 0.0);
    }

    #[test]
    fn the_hint_needs_the_piece_to_be_ahead_close_and_in_line() {
        // Too far (lookahead 80 m), already on the piece, past it, or beside it.
        assert_eq!(hint_at(10.0, 0.0, 8.0), 0.0);
        assert_eq!(hint_at(101.0, 0.0, 8.0), 0.0);
        assert_eq!(hint_at(130.0, 0.0, 8.0), 0.0);
        assert_eq!(hint_at(60.0, 20.0, 8.0), 0.0);
        // Just inside the lookahead.
        assert_eq!(hint_at(100.0 - APPROACH_LOOKAHEAD_M + 1.0, 0.0, 8.0), 12.0);
        // Driving away from the piece (nose turned more than 60 degrees).
        let tracker = TraversalTracker::for_course(&course_with_ramp(), limits());
        let mut away = level_car_at(60.0, 0.0);
        away.velocity_m_s = [-8.0, 0.0, 0.0];
        away.nose_world = [-1.0, 0.0, 0.0];
        assert_eq!(tracker.approach_hint_m_s(&away), 0.0);
    }

    #[test]
    fn a_piece_that_takes_any_speed_gives_no_hint() {
        let tracker = TraversalTracker::empty();
        assert_eq!(tracker.approach_hint_m_s(&level_car_at(0.0, 0.0)), 0.0);
    }

    #[test]
    fn the_hint_is_off_while_a_record_is_active() {
        let mut tracker = TraversalTracker::for_course(&course_with_ramp(), limits());
        tracker.update(0, 1.0 / 120.0, &level_car_at(99.0, 0.0));
        tracker.update(1, 1.0 / 120.0, &level_car_at(101.0, 0.0));
        assert!(tracker.active().is_some());
        assert_eq!(tracker.approach_hint_m_s(&level_car_at(101.0, 0.0)), 0.0);
    }

    #[test]
    fn the_record_ends_past_the_piece() {
        let mut tracker = TraversalTracker::for_course(&course_with_ramp(), limits());
        for (tick, x_m) in [99.0, 101.0, 110.0, 121.0].into_iter().enumerate() {
            tracker.update(tick as u64, 1.0 / 120.0, &level_car_at(x_m, 0.0));
        }

        assert!(tracker.active().is_none());
        assert_eq!(tracker.finished().len(), 1);
        assert_eq!(tracker.finished()[0].phase, TraversalPhase::Exited);
    }

    #[test]
    fn the_record_times_out() {
        let mut tracker = TraversalTracker::for_course(&course_with_ramp(), limits());
        tracker.update(0, 1.0 / 120.0, &level_car_at(99.0, 0.0));
        tracker.update(1, 1.0 / 120.0, &level_car_at(101.0, 0.0));
        // 20 m at 30 m/s plus 3 s: long gone after 4 s while still on the piece.
        let late_tick = 4 * 120 + 2;
        tracker.update(late_tick, 1.0 / 120.0, &level_car_at(101.0, 0.0));

        assert!(tracker.active().is_none());
        assert_eq!(tracker.finished().len(), 1);
    }
}
