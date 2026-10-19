//! Turns a checked course (`course::CourseSpec`) into the shapes the world collides with
//! and the renderer draws: floor segments split by gaps, ramps and walls as convex hulls
//! and boxes, plus non-solid markings (landing pads, checkpoint and finish lines, kick
//! plates). The game and the sim build from this one list, so what you see is what the
//! car hits (SPEC §7.11).

use rapier3d::prelude::*;
use serde::Serialize;

use crate::course::{CourseSpec, PieceSpec, Side};
use crate::world::Pose;

/// Course width across the lane (local Z) where a piece does not give one, in m.
const LANE_WIDTH_M: f32 = 16.0;
/// Floor thickness below y = 0, in m.
const FLOOR_THICKNESS_M: f32 = 1.0;
/// SPEC §7.11 car row: each parked car is 4.5 m long, 1.9 m wide, 0.6 m apart.
pub const PARKED_CAR_LENGTH_M: f32 = 4.5;
pub const PARKED_CAR_WIDTH_M: f32 = 1.9;
pub const PARKED_CAR_GAP_M: f32 = 0.6;
/// Bumps: one every this many metres, each this long (half up, half down).
pub const BUMP_SPACING_M: f32 = 6.0;
const BUMP_LENGTH_M: f32 = 5.0;
/// Ramp, kicker and bump hulls reach this far below the floor (hidden by it), so even a
/// nearly flat segment is a solid Rapier can always build a hull from, in m.
const HULL_BASE_DEPTH_M: f32 = 0.2;
/// Ramps are built from this many convex segments of rising slope.
const RAMP_SEGMENTS: usize = 6;
/// Walls are this thick along the course.
pub const WALL_THICKNESS_M: f32 = 1.0;
/// A wall's jump ramp: this long and steep, ending this far before the wall. At 30°
/// the curved ramp's lip is about 3.2 m high, above a 3 m wall.
pub const WALL_RAMP_LENGTH_M: f32 = 10.0;
const WALL_RAMP_ANGLE_DEG: f32 = 30.0;
pub const WALL_RAMP_GAP_M: f32 = 2.0;
/// Halfpipe walls: flat floor this wide between the curves, wall panels this thick, and
/// this many flat panels per quarter circle.
const HALFPIPE_FLOOR_WIDTH_M: f32 = 4.0;
const HALFPIPE_PANEL_THICKNESS_M: f32 = 0.3;
const HALFPIPE_PANELS: usize = 8;
/// Markings: landing pads, kick plates and the checkpoint/finish lines, in m along the
/// course; drawn this thick above the floor.
const LANDING_PAD_LENGTH_M: f32 = 20.0;
pub const KICK_PLATE_LENGTH_M: f32 = 4.0;
const LINE_LENGTH_M: f32 = 0.5;
/// Checkpoint and finish lines are wider than the lane, so a car can't drive around
/// them on a track up to this wide, in m.
const LINE_WIDTH_M: f32 = 20.0;
const MARKING_THICKNESS_M: f32 = 0.02;

/// What a shape is for, so the renderer can colour it (SPEC §11.4: plain colours).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// The ground at y = 0.
    Floor,
    /// Anything the car drives up: ramps, kickers, halfpipe walls.
    Ramp,
    /// Things to jump over: parked cars, bumps, walls.
    Obstacle,
    /// Where a big jump should land (a marking).
    LandingPad,
    /// Pushes the car once per pass (a marking).
    KickPlate,
    /// Moves the respawn point (a marking).
    Checkpoint,
    /// Ends the run (a marking).
    Finish,
}

/// A shape's geometry in its own frame. As JSON: `{"box": {"half_extents_m": …}}` or
/// `{"hull": {"points_m": …}}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeGeometry {
    /// A box with these half-sizes, in m.
    Box {
        /// Half the size along each local axis, in m.
        half_extents_m: [f32; 3],
    },
    /// The convex hull of these points, in m.
    Hull {
        /// Corner points in the shape's frame, in m.
        points_m: Vec<[f32; 3]>,
    },
}

/// One shape of the course, placed in the world.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CourseShape {
    /// The piece it belongs to (index into `CourseSpec::pieces`); None for the floor.
    pub piece: Option<usize>,
    /// What it is, for colouring.
    pub surface: Surface,
    /// Its size and form in its own frame.
    pub geometry: ShapeGeometry,
    /// Where the shape's own frame sits and how it is turned, in world coordinates.
    pub pose: Pose,
    /// False for markings the car drives over without touching.
    pub solid: bool,
}

/// How long a row of `count` parked cars is along the course, gaps included, in m.
pub fn car_row_length_m(count: u32) -> f32 {
    let count = count as f32;
    count * PARKED_CAR_LENGTH_M + (count - 1.0).max(0.0) * PARKED_CAR_GAP_M
}

/// Every shape of a course, floor first.
pub fn build_course_shapes(spec: &CourseSpec) -> Vec<CourseShape> {
    let mut shapes = floor_segments(spec);
    for (index, piece) in spec.pieces.iter().enumerate() {
        shapes.extend(piece_shapes(index, piece));
    }
    shapes
}

/// The Rapier collider for a solid shape (None for markings, or for a hull Rapier
/// cannot build, which cannot happen for the hulls built here).
pub fn shape_collider(shape: &CourseShape, friction: f32) -> Option<ColliderBuilder> {
    if !shape.solid {
        return None;
    }
    let builder = match &shape.geometry {
        ShapeGeometry::Box {
            half_extents_m: [x, y, z],
        } => ColliderBuilder::cuboid(*x, *y, *z),
        ShapeGeometry::Hull { points_m } => {
            let points: Vec<Vector> = points_m
                .iter()
                .map(|point| Vector::from_array(*point))
                .collect();
            ColliderBuilder::convex_hull(&points)?
        }
    };
    Some(builder.position(shape.pose.to_rapier()).friction(friction))
}

/// The floor: the bounds' footprint at y = 0, cut into segments along X around gaps.
fn floor_segments(spec: &CourseSpec) -> Vec<CourseShape> {
    let [min_x, _, min_z] = spec.bounds.min;
    let [max_x, _, max_z] = spec.bounds.max;
    let mut holes: Vec<(f32, f32)> = Vec::new();
    for piece in &spec.pieces {
        if let PieceSpec::Gap { pos, length } = piece {
            holes.push((pos[0], pos[0] + length));
        }
    }
    holes.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut segments = Vec::new();
    let mut from_x = min_x;
    for (hole_start, hole_end) in holes {
        segments.push((from_x, hole_start.max(from_x)));
        from_x = from_x.max(hole_end);
    }
    segments.push((from_x, max_x));
    segments
        .into_iter()
        .filter(|(start, end)| end - start > 0.01)
        .map(|(start, end)| {
            let half = [
                (end - start) / 2.0,
                FLOOR_THICKNESS_M / 2.0,
                (max_z - min_z) / 2.0,
            ];
            let centre = [
                (start + end) / 2.0,
                -FLOOR_THICKNESS_M / 2.0,
                (min_z + max_z) / 2.0,
            ];
            floor_box(centre, half)
        })
        .collect()
}

/// The shapes of one piece.
fn piece_shapes(index: usize, piece: &PieceSpec) -> Vec<CourseShape> {
    let at = |pos: [f32; 3], yaw: f32| PieceFrame::new(index, pos, yaw);
    match piece {
        PieceSpec::Ramp {
            pos,
            yaw,
            length,
            width,
            angle_deg,
        } => at(*pos, *yaw).ramp(Surface::Ramp, 0.0, 0.0, *length, *width, *angle_deg),
        PieceSpec::Kicker {
            pos,
            yaw,
            length,
            width,
            angle_deg,
            side,
        } => {
            // Left is −Z (SPEC §4): the kicker rises away from the lane's centre line.
            let outer_z = match side {
                Side::Left => -width,
                Side::Right => *width,
            };
            at(*pos, *yaw).kicker(*length, outer_z, *angle_deg)
        }
        PieceSpec::Gap { .. } => Vec::new(),
        PieceSpec::CarRow {
            pos,
            yaw,
            count,
            height,
            ..
        } => car_row(at(*pos, *yaw), *count, *height),
        PieceSpec::Halfpipe {
            pos,
            yaw,
            radius,
            length,
        } => halfpipe(at(*pos, *yaw), *radius, *length),
        PieceSpec::Bumps {
            pos,
            yaw,
            count,
            height,
        } => bumps(at(*pos, *yaw), *count, *height),
        PieceSpec::KickPlate { pos, yaw, .. } => {
            vec![at(*pos, *yaw).marking(Surface::KickPlate, KICK_PLATE_LENGTH_M, LANE_WIDTH_M)]
        }
        PieceSpec::Wall {
            pos,
            yaw,
            height,
            with_ramp,
        } => wall(at(*pos, *yaw), *height, *with_ramp),
        PieceSpec::LandingPad { pos, yaw } => {
            vec![at(*pos, *yaw).marking(Surface::LandingPad, LANDING_PAD_LENGTH_M, LANE_WIDTH_M)]
        }
        PieceSpec::Checkpoint { pos, yaw } => {
            vec![at(*pos, *yaw).marking(Surface::Checkpoint, LINE_LENGTH_M, LINE_WIDTH_M)]
        }
        PieceSpec::Finish { pos, yaw } => {
            vec![at(*pos, *yaw).marking(Surface::Finish, LINE_LENGTH_M, LINE_WIDTH_M)]
        }
    }
}

/// SPEC §7.11: `count` parked cars nose to tail along local +X (PLAN §3.2: five cars
/// make a row about 25 m long).
fn car_row(frame: PieceFrame, count: u32, height: f32) -> Vec<CourseShape> {
    let half = [
        PARKED_CAR_LENGTH_M / 2.0,
        height / 2.0,
        PARKED_CAR_WIDTH_M / 2.0,
    ];
    let mut shapes = Vec::new();
    for car in 0..count {
        let start_x = car as f32 * (PARKED_CAR_LENGTH_M + PARKED_CAR_GAP_M);
        let centre = [start_x + half[0], half[1], 0.0];
        shapes.push(frame.local_box(Surface::Obstacle, centre, half, 0.0));
    }
    shapes
}

/// Gentle humps across the lane, one every `BUMP_SPACING_M`. Each is
/// `BUMP_LENGTH_M` long, so its faces are shallow enough that the wheels lift the car
/// before the chassis nose (0.14 m above the ground) reaches the hump.
fn bumps(frame: PieceFrame, count: u32, height: f32) -> Vec<CourseShape> {
    let half_width = LANE_WIDTH_M / 2.0;
    let mut shapes = Vec::new();
    for bump in 0..count {
        let start_x = bump as f32 * BUMP_SPACING_M;
        let mut points = Vec::new();
        for z in [-half_width, half_width] {
            points.push([start_x, -HULL_BASE_DEPTH_M, z]);
            points.push([start_x, 0.0, z]);
            points.push([start_x + BUMP_LENGTH_M / 2.0, height, z]);
            points.push([start_x + BUMP_LENGTH_M, 0.0, z]);
            points.push([start_x + BUMP_LENGTH_M, -HULL_BASE_DEPTH_M, z]);
        }
        shapes.push(frame.hull(Surface::Obstacle, points));
    }
    shapes
}

/// A wall across the lane, with an optional jump ramp ending just before it.
fn wall(frame: PieceFrame, height: f32, with_ramp: bool) -> Vec<CourseShape> {
    let half = [WALL_THICKNESS_M / 2.0, height / 2.0, LANE_WIDTH_M / 2.0];
    let mut shapes = vec![frame.local_box(Surface::Obstacle, [half[0], half[1], 0.0], half, 0.0)];
    if with_ramp {
        let start_x = -WALL_RAMP_GAP_M - WALL_RAMP_LENGTH_M;
        shapes.extend(frame.ramp(
            Surface::Ramp,
            start_x,
            0.0,
            WALL_RAMP_LENGTH_M,
            LANE_WIDTH_M,
            WALL_RAMP_ANGLE_DEG,
        ));
    }
    shapes
}

/// A U-channel along local +X: a flat floor between two quarter-circle walls, each made
/// of `HALFPIPE_PANELS` flat panels.
fn halfpipe(frame: PieceFrame, radius: f32, length: f32) -> Vec<CourseShape> {
    let mut shapes = Vec::new();
    let step_rad = std::f32::consts::FRAC_PI_2 / HALFPIPE_PANELS as f32;
    for side in [-1.0_f32, 1.0] {
        let centre_z = side * HALFPIPE_FLOOR_WIDTH_M / 2.0;
        for panel in 0..HALFPIPE_PANELS {
            // The panel joins two arc points, at these angles from straight down.
            let start_rad = panel as f32 * step_rad;
            let (z0, y0) = arc_point(centre_z, side, radius, start_rad);
            let (z1, y1) = arc_point(centre_z, side, radius, start_rad + step_rad);
            let chord_m = ((z1 - z0).powi(2) + (y1 - y0).powi(2)).sqrt();
            let tilt_rad = libm::atan2f(y1 - y0, z1 - z0);
            let half = [
                length / 2.0,
                HALFPIPE_PANEL_THICKNESS_M / 2.0,
                chord_m / 2.0,
            ];
            // Push the panel away from the arc's centre by half its thickness, so its
            // inner face (where the wheels ride) lies on the arc, flush with the floor.
            let mid_z = (z0 + z1) / 2.0;
            let mid_y = (y0 + y1) / 2.0;
            let out_z = mid_z - centre_z;
            let out_y = mid_y - radius;
            let out_length = (out_z * out_z + out_y * out_y).sqrt();
            let push_m = HALFPIPE_PANEL_THICKNESS_M / 2.0 / out_length;
            let centre = [length / 2.0, mid_y + out_y * push_m, mid_z + out_z * push_m];
            // The panel's long side (local Z) follows the chord: a roll about local +X.
            shapes.push(frame.local_box(Surface::Ramp, centre, half, -tilt_rad));
        }
    }
    shapes
}

/// The four corners of one side (at `z`) of a ramp segment: its top edge from
/// (`x0`, `h0`) to (`x1`, `h1`) and its base `HULL_BASE_DEPTH_M` below the floor.
fn segment_side(x0: f32, h0: f32, x1: f32, h1: f32, z: f32) -> [[f32; 3]; 4] {
    [
        [x0, -HULL_BASE_DEPTH_M, z],
        [x0, h0, z],
        [x1, -HULL_BASE_DEPTH_M, z],
        [x1, h1, z],
    ]
}

/// A ramp's side profile as (distance along, height) points, in m: `RAMP_SEGMENTS`
/// straight segments, each steeper than the last by the same step, ending at
/// `lip_angle_deg`. So a fast car meets no sharp kink (a straight wedge's foot crashed
/// the car above about 25 m/s), and the ramp ends lower than `length · tan(angle)`.
fn ramp_profile(length: f32, lip_angle_deg: f32) -> Vec<(f32, f32)> {
    let segment_length_m = length / RAMP_SEGMENTS as f32;
    let mut profile = vec![(0.0, 0.0)];
    let mut height_m = 0.0;
    for segment in 1..=RAMP_SEGMENTS {
        let slope_deg = lip_angle_deg * segment as f32 / RAMP_SEGMENTS as f32;
        height_m += segment_length_m * libm::tanf(slope_deg.to_radians());
        profile.push((segment as f32 * segment_length_m, height_m));
    }
    profile
}

/// A point on a halfpipe wall's quarter circle, `angle_rad` up from its lowest point,
/// as (z, y) in m; the circle's centre is `radius` above (`centre_z`, 0).
fn arc_point(centre_z: f32, side: f32, radius: f32, angle_rad: f32) -> (f32, f32) {
    let z = centre_z + side * radius * libm::sinf(angle_rad);
    let y = radius - radius * libm::cosf(angle_rad);
    (z, y)
}

/// A piece's frame: its start point and its heading (yaw about +Y).
struct PieceFrame {
    index: usize,
    position: Vector,
    rotation: Rotation,
}

impl PieceFrame {
    fn new(index: usize, position_m: [f32; 3], yaw_deg: f32) -> PieceFrame {
        PieceFrame {
            index,
            position: Vector::from_array(position_m),
            rotation: Rotation::from_rotation_y(yaw_deg.to_radians()),
        }
    }

    /// A ramp starting `start_x` along the piece, centred `offset_z` across it, `length`
    /// long and `width` wide, curving up along +X from flat to `lip_angle_deg` at its
    /// sheer back (see `ramp_profile`): one convex piece per profile segment.
    fn ramp(
        &self,
        surface: Surface,
        start_x: f32,
        offset_z: f32,
        length: f32,
        width: f32,
        lip_angle_deg: f32,
    ) -> Vec<CourseShape> {
        let near_z = offset_z - width / 2.0;
        let far_z = offset_z + width / 2.0;
        self.profile_hulls(surface, length, lip_angle_deg, |x0, h0, x1, h1| {
            let mut points = Vec::new();
            for z in [near_z, far_z] {
                points.extend(segment_side(start_x + x0, h0, start_x + x1, h1, z));
            }
            points
        })
    }

    /// A kicker: a ramp beside the lane's centre line (z = 0) out to `outer_z`, whose
    /// top also slopes across, from 0 at the centre line to the full ramp height at
    /// `outer_z`. So the wheels on that side ride up and the car rolls, with no wall
    /// under the car for the chassis to jam against.
    fn kicker(&self, length: f32, outer_z: f32, lip_angle_deg: f32) -> Vec<CourseShape> {
        self.profile_hulls(Surface::Ramp, length, lip_angle_deg, |x0, h0, x1, h1| {
            let mut points = segment_side(x0, 0.0, x1, 0.0, 0.0).to_vec();
            points.extend(segment_side(x0, h0, x1, h1, outer_z));
            points
        })
    }

    /// One convex hull per segment of a ramp profile; `segment_points` gives a
    /// segment's corners from its start (x0, h0) and end (x1, h1), in m.
    fn profile_hulls(
        &self,
        surface: Surface,
        length: f32,
        lip_angle_deg: f32,
        segment_points: impl Fn(f32, f32, f32, f32) -> Vec<[f32; 3]>,
    ) -> Vec<CourseShape> {
        let profile = ramp_profile(length, lip_angle_deg);
        let mut shapes = Vec::new();
        for pair in profile.windows(2) {
            let [(x0, h0), (x1, h1)] = [pair[0], pair[1]];
            shapes.push(self.hull(surface, segment_points(x0, h0, x1, h1)));
        }
        shapes
    }

    /// A flat non-solid marking centred on the lane, `length` long from the piece start
    /// and `width` across, in m.
    fn marking(&self, surface: Surface, length: f32, width: f32) -> CourseShape {
        let half = [length / 2.0, MARKING_THICKNESS_M / 2.0, width / 2.0];
        let mut shape = self.local_box(surface, [half[0], half[1], 0.0], half, 0.0);
        shape.solid = false;
        shape
    }

    /// A convex hull of points given in the piece frame.
    fn hull(&self, surface: Surface, points_m: Vec<[f32; 3]>) -> CourseShape {
        self.shape(
            surface,
            ShapeGeometry::Hull { points_m },
            Vector::ZERO,
            Rotation::IDENTITY,
        )
    }

    /// A box centred at `centre_m` in the piece frame, rolled `roll_rad` about local +X.
    fn local_box(
        &self,
        surface: Surface,
        centre_m: [f32; 3],
        half_extents_m: [f32; 3],
        roll_rad: f32,
    ) -> CourseShape {
        let geometry = ShapeGeometry::Box { half_extents_m };
        self.shape(
            surface,
            geometry,
            Vector::from_array(centre_m),
            Rotation::from_rotation_x(roll_rad),
        )
    }

    /// A shape whose own frame sits at `offset`, turned by `turn`, inside the piece frame.
    fn shape(
        &self,
        surface: Surface,
        geometry: ShapeGeometry,
        offset: Vector,
        turn: Rotation,
    ) -> CourseShape {
        let position = self.position + self.rotation * offset;
        let rotation = self.rotation * turn;
        CourseShape {
            piece: Some(self.index),
            surface,
            geometry,
            pose: Pose::new(position, rotation),
            solid: true,
        }
    }
}

/// An unrotated floor box.
fn floor_box(centre_m: [f32; 3], half_extents_m: [f32; 3]) -> CourseShape {
    CourseShape {
        piece: None,
        surface: Surface::Floor,
        geometry: ShapeGeometry::Box { half_extents_m },
        pose: Pose::new(Vector::from_array(centre_m), Rotation::IDENTITY),
        solid: true,
    }
}
