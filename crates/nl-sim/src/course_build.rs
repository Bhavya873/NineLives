//! Turns a checked course (`course::CourseSpec`) into the shapes the world collides with
//! and the renderer draws: floor segments split by gaps, ramps and walls as convex hulls
//! and boxes, plus non-solid markings (landing pads, checkpoint and finish lines, kick
//! plates). The game and the sim build from this one list, so what you see is what the
//! car hits (SPEC §7.11).

use rapier3d::prelude::*;
use serde::Serialize;

use crate::course::{CourseSpec, PieceSpec, Side};
use crate::course_mesh::{LoftedMesh, loft};
use crate::world::Pose;

/// Course width across the lane (local Z) where a piece does not give one, in m.
pub const LANE_WIDTH_M: f32 = 16.0;
/// Floor thickness below y = 0, in m.
const FLOOR_THICKNESS_M: f32 = 1.0;
/// SPEC §7.11 car row: each parked car is 4.5 m long, 1.9 m wide, 0.6 m apart.
pub const PARKED_CAR_LENGTH_M: f32 = 4.5;
pub const PARKED_CAR_WIDTH_M: f32 = 1.9;
pub const PARKED_CAR_GAP_M: f32 = 0.6;
/// Bumps: each is as long as its faces need to be shallow, and they are this far apart
/// (flat floor between one and the next), in m. A bump `height` tall is `height /
/// BUMP_FACE_SLOPE * 2` long: R2c found that a car at 40 to 55 m/s leaves a steeper face
/// (a 0.4 m bump of 5 m is 9°) with a vertical speed of 7 to 8 m/s and lands on the next
/// bump too hard; a 0.2 m bump of 5 m (slope 0.08, 4.6°) never crashed it.
pub const BUMP_FACE_SLOPE: f32 = 0.08;
const BUMP_GAP_M: f32 = 1.0;
/// Shortest bump, in m (a 0.2 m bump is 5 m long).
const BUMP_MIN_LENGTH_M: f32 = 5.0;
/// Ramp, kicker and bump hulls reach this far below the floor (hidden by it), so even a
/// nearly flat segment is a solid Rapier can always build a hull from, in m.
const HULL_BASE_DEPTH_M: f32 = 0.2;
/// A ramp is built from at least this many straight segments of rising slope (more for
/// steeper ramps, see `MAX_KINK_DEG`).
const RAMP_SEGMENTS: usize = 6;
/// The angle between two neighbouring segments of a curved piece stays at or under this,
/// in degrees (docs/contracts/ramp-curvature.md allows 1.5°; this keeps rounding margin),
/// so a wheel ray at 100 m/s never meets a visible kink.
pub const MAX_KINK_DEG: f32 = 1.2;
/// A corkscrew's edges are sampled at least this many steps along its length, so its twist
/// looks and drives smooth (more for steeper lips, see `MAX_KINK_DEG`).
const CORKSCREW_SAMPLES: usize = 16;
/// The concave entry radius of the curved pieces' foot, in m (docs/contracts/
/// ramp-curvature.md): R ≥ v² / (n_max − g) at the design speed 55.6 m/s. R2b measured
/// the suspension with its bump stop clean up to 4.2 g (R = 99 m); 110 m is 3.9 g, a
/// 7% margin under that. `configs/pieces.json` holds the same number for generated
/// courses (a test keeps them equal).
pub const MIN_CONCAVE_RADIUS_M: f32 = 110.0;
/// A catch pit's floor slopes down from the lip at this angle and the exit slopes up at
/// the other, in degrees (docs/contracts/survival.md: a car that falls short drives out).
const PIT_ENTRY_SLOPE_DEG: f32 = 30.0;
const PIT_EXIT_SLOPE_DEG: f32 = 8.0;
/// A pit's mesh reaches this far below its deepest point, in m.
const PIT_BASE_DEPTH_M: f32 = 0.2;
/// Walls are this thick along the course.
pub const WALL_THICKNESS_M: f32 = 1.0;
/// A wall with a ramp is a table top (docs/contracts/survival.md: not clearing the wall
/// means stopping on the ramp, never hitting its face): a curved ramp of radius
/// `MIN_CONCAVE_RADIUS_M` up to the wall's height, a flat deck as long as the wall is
/// thick, and the same curve back down. A car too slow to jump drives over it.
/// Halfpipe walls: flat floor this wide between the curves, wall panels this thick, and
/// this many flat panels per quarter circle.
const HALFPIPE_FLOOR_WIDTH_M: f32 = 4.0;
const HALFPIPE_WALL_THICKNESS_M: f32 = 0.3;
const HALFPIPE_ARC_SEGMENTS: usize = 16;
/// Half loop: the road shell is this thick, and the half circle has this many facets.
const HALF_LOOP_THICKNESS_M: f32 = 0.3;
const HALF_LOOP_ARC_SEGMENTS: usize = 48;
/// The half loop's road is slick where the body scrapes it. A circular loop pulls about
/// 6 g at the bottom, past what the suspension holds, so the body rides on the road; at
/// ground friction 1.0 that rubbing stopped the car on the wall instead of at the top.
const HALF_LOOP_FRICTION: f32 = 0.1;
/// Markings: landing pads, kick plates and the checkpoint/finish lines, in m along the
/// course; drawn this thick above the floor.
const LANDING_PAD_LENGTH_M: f32 = 20.0;
pub const KICK_PLATE_LENGTH_M: f32 = 4.0;
const LINE_LENGTH_M: f32 = 0.5;
/// Checkpoint and finish lines are wider than the lane, so a car can't drive around
/// them on a track up to this wide, in m.
const LINE_WIDTH_M: f32 = 20.0;
const MARKING_THICKNESS_M: f32 = 0.02;
/// Rapier `user_data` on the colliders of obstacles (car rows, bumps, walls), so the
/// sim can tell the chassis hitting one from it touching a ramp or the floor.
pub const OBSTACLE_USER_DATA: u128 = 1;

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
    /// A closed triangle mesh (curved pieces: ramps, kickers, halfpipe walls), in m.
    Mesh {
        /// Corner points in the shape's frame, in m.
        points_m: Vec<[f32; 3]>,
        /// Triangles as indices into `points_m`, counter-clockwise seen from outside.
        triangles: Vec<[u32; 3]>,
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
    /// Its own friction where the car's body scrapes it; None uses the course's.
    #[serde(skip)]
    pub friction: Option<f32>,
}

/// How long a bump `height_m` tall is along the course, in m: both faces slope at
/// `BUMP_FACE_SLOPE` or less.
pub fn bump_length_m(height_m: f32) -> f32 {
    (2.0 * height_m / BUMP_FACE_SLOPE).max(BUMP_MIN_LENGTH_M)
}

/// The distance from the start of one bump to the start of the next, in m.
pub fn bump_spacing_m(height_m: f32) -> f32 {
    bump_length_m(height_m) + BUMP_GAP_M
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
        ShapeGeometry::Mesh {
            points_m,
            triangles,
        } => {
            let points: Vec<Vector> = points_m
                .iter()
                .map(|point| Vector::from_array(*point))
                .collect();
            // Internal-edge fixing keeps contacts on the smooth surface, so the chassis
            // slides over the joints between triangles instead of catching on them.
            let flags = TriMeshFlags::ORIENTED | TriMeshFlags::FIX_INTERNAL_EDGES;
            ColliderBuilder::trimesh_with_flags(points, triangles.clone(), flags).ok()?
        }
    };
    let user_data = if shape.surface == Surface::Obstacle {
        OBSTACLE_USER_DATA
    } else {
        0
    };
    // The floor's contacts go through `floor_hook`, which keeps their normal vertical.
    let hooks = if shape.surface == Surface::Floor {
        ActiveHooks::MODIFY_SOLVER_CONTACTS
    } else {
        ActiveHooks::empty()
    };
    Some(
        builder
            .position(shape.pose.to_rapier())
            .friction(shape.friction.unwrap_or(friction))
            .user_data(user_data)
            .active_hooks(hooks),
    )
}

/// The floor: the bounds' footprint at y = 0, cut into segments along X around gaps.
fn floor_segments(spec: &CourseSpec) -> Vec<CourseShape> {
    let [min_x, _, min_z] = spec.bounds.min;
    let [max_x, _, max_z] = spec.bounds.max;
    let mut holes: Vec<(f32, f32)> = Vec::new();
    for piece in &spec.pieces {
        if let PieceSpec::Gap { pos, length, .. } = piece {
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
            ..
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
        PieceSpec::Corkscrew {
            pos,
            yaw,
            length,
            width,
            angle_deg,
            bank_deg,
            side,
        } => {
            // Left is −Z (SPEC §4): the raised edge is on that side of the lane.
            let raised_side_z = match side {
                Side::Left => -1.0,
                Side::Right => 1.0,
            };
            let twist = CorkscrewTwist {
                length: *length,
                width: *width,
                lip_angle_deg: *angle_deg,
                bank_deg: *bank_deg,
            };
            at(*pos, *yaw).corkscrew(&twist, raised_side_z)
        }
        PieceSpec::Gap {
            pos,
            length,
            pit_depth,
            entry_height,
        } => catch_pit(at(*pos, 0.0), *length, *pit_depth, *entry_height),
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
        PieceSpec::HalfLoop {
            pos,
            yaw,
            radius,
            width,
            ..
        } => half_loop(at(*pos, *yaw), *radius, *width),
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

/// The catch pit of a gap (docs/contracts/survival.md): a mesh filling the hole with a
/// sloped floor, from the lip's height `entry_height_m` down `PIT_ENTRY_SLOPE_DEG` to
/// `depth_m` below the floor, flat, then up `PIT_EXIT_SLOPE_DEG` to the floor at the
/// far edge. No pit (`depth_m` 0) leaves the bare hole.
fn catch_pit(
    frame: PieceFrame,
    length_m: f32,
    depth_m: f32,
    entry_height_m: f32,
) -> Vec<CourseShape> {
    if depth_m <= 0.0 {
        return Vec::new();
    }
    let mut run_in_m = (entry_height_m + depth_m) / libm::tanf(PIT_ENTRY_SLOPE_DEG.to_radians());
    let mut run_out_m = depth_m / libm::tanf(PIT_EXIT_SLOPE_DEG.to_radians());
    if run_in_m + run_out_m > length_m {
        // A short pit: the same shape, squeezed (steeper slopes, no flat bottom).
        let squeeze = length_m / (run_in_m + run_out_m);
        run_in_m *= squeeze;
        run_out_m *= squeeze;
    }
    let profile = [
        (0.0, entry_height_m),
        (run_in_m, -depth_m),
        (length_m - run_out_m, -depth_m),
        (length_m, 0.0),
    ];
    let section = |z: f32| {
        // Lower chain: a flat base under the deepest point; upper chain: the pit floor.
        let base_m = -depth_m - PIT_BASE_DEPTH_M;
        let mut points: Vec<[f32; 3]> = profile.iter().map(|(x, _)| [*x, base_m, z]).collect();
        points.extend(profile.iter().rev().map(|(x, y)| [*x, *y, z]));
        points
    };
    let near = section(-LANE_WIDTH_M / 2.0);
    let far = section(LANE_WIDTH_M / 2.0);
    vec![frame.mesh(Surface::Ramp, loft(&near, &far))]
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

/// Gentle humps across the lane, one every `bump_spacing_m(height)`. Each is
/// `bump_length_m(height)` long, so its faces are shallow enough that the wheels lift the
/// car before the chassis nose (0.14 m above the ground) reaches the hump, and a fast car
/// leaves a face with a small vertical speed.
fn bumps(frame: PieceFrame, count: u32, height: f32) -> Vec<CourseShape> {
    let half_width = LANE_WIDTH_M / 2.0;
    let mut shapes = Vec::new();
    let length_m = bump_length_m(height);
    for bump in 0..count {
        let start_x = bump as f32 * bump_spacing_m(height);
        let mut points = Vec::new();
        for z in [-half_width, half_width] {
            points.push([start_x, -HULL_BASE_DEPTH_M, z]);
            points.push([start_x, 0.0, z]);
            points.push([start_x + length_m / 2.0, height, z]);
            points.push([start_x + length_m, 0.0, z]);
            points.push([start_x + length_m, -HULL_BASE_DEPTH_M, z]);
        }
        shapes.push(frame.hull(Surface::Obstacle, points));
    }
    shapes
}

/// A wall across the lane, or with `with_ramp` a table top of its height (see
/// `table_top_profile`).
fn wall(frame: PieceFrame, height: f32, with_ramp: bool) -> Vec<CourseShape> {
    if with_ramp {
        // A table top starting at the piece's position (see `table_top_profile`).
        let profile = table_top_profile(height);
        let section = |z: f32| profile_section(&profile, 0.0, z, 1.0);
        let near = section(-LANE_WIDTH_M / 2.0);
        let far = section(LANE_WIDTH_M / 2.0);
        return vec![frame.mesh(Surface::Ramp, loft(&near, &far))];
    }
    let half = [WALL_THICKNESS_M / 2.0, height / 2.0, LANE_WIDTH_M / 2.0];
    vec![frame.local_box(Surface::Obstacle, [half[0], half[1], 0.0], half, 0.0)]
}

/// A U-channel along local +X: a flat floor between two quarter-circle walls, each one
/// smooth mesh of `HALFPIPE_ARC_SEGMENTS` facets whose inner face lies on the arc, flush
/// with the floor.
fn halfpipe(frame: PieceFrame, radius: f32, length: f32) -> Vec<CourseShape> {
    let mut shapes = Vec::new();
    for side in [-1.0_f32, 1.0] {
        let centre_z = side * HALFPIPE_FLOOR_WIDTH_M / 2.0;
        let section = |x: f32| {
            // Lower chain: the wall's outer face; upper chain (reversed): its inner face.
            let outer_radius = radius + HALFPIPE_WALL_THICKNESS_M;
            let mut points = arc_points(x, centre_z, side, radius, outer_radius);
            let mut inner = arc_points(x, centre_z, side, radius, radius);
            inner.reverse();
            points.extend(inner);
            points
        };
        shapes.push(frame.mesh(Surface::Ramp, loft(&section(0.0), &section(length))));
    }
    shapes
}

/// The first half of a loop along local +X: one smooth shell of
/// `HALF_LOOP_ARC_SEGMENTS` facets whose inner face is a half circle of `radius` around
/// (0, `radius`), from flush with the floor at x = 0 to upside down at the top.
fn half_loop(frame: PieceFrame, radius: f32, width: f32) -> Vec<CourseShape> {
    let section = |z: f32| {
        // Lower chain: the shell's outer face; upper chain (reversed): the road.
        let mut points = half_loop_arc(z, radius, radius + HALF_LOOP_THICKNESS_M);
        let mut road = half_loop_arc(z, radius, radius);
        road.reverse();
        points.extend(road);
        points
    };
    // Lofted toward −Z, so the facets face outward as the halfpipe's do.
    let mut road = frame.mesh(
        Surface::Ramp,
        loft(&section(width / 2.0), &section(-width / 2.0)),
    );
    road.friction = Some(HALF_LOOP_FRICTION);
    vec![road]
}

/// Points at `z` on a half circle of `arc_radius` around (0, `radius`), from straight
/// below the centre (the bottom) up the +X side to straight above it, as (x, y, z).
fn half_loop_arc(z: f32, radius: f32, arc_radius: f32) -> Vec<[f32; 3]> {
    let step_rad = std::f32::consts::PI / HALF_LOOP_ARC_SEGMENTS as f32;
    let mut points = Vec::new();
    for segment in 0..=HALF_LOOP_ARC_SEGMENTS {
        let angle_rad = segment as f32 * step_rad;
        let x = arc_radius * libm::sinf(angle_rad);
        let y = radius - arc_radius * libm::cosf(angle_rad);
        points.push([x, y, z]);
    }
    points
}

/// Points at `x` on an arc of `arc_radius` around the halfpipe wall's centre (`radius`
/// above (`centre_z`, 0)), from straight down to level with the centre, as (x, y, z).
fn arc_points(x: f32, centre_z: f32, side: f32, radius: f32, arc_radius: f32) -> Vec<[f32; 3]> {
    let step_rad = std::f32::consts::FRAC_PI_2 / HALFPIPE_ARC_SEGMENTS as f32;
    let mut points = Vec::new();
    for segment in 0..=HALFPIPE_ARC_SEGMENTS {
        let angle_rad = segment as f32 * step_rad;
        let z = centre_z + side * arc_radius * libm::sinf(angle_rad);
        let y = radius - arc_radius * libm::cosf(angle_rad);
        points.push([x, y, z]);
    }
    points
}

/// A ramp's cross-section at `z` for `loft`: the base `HULL_BASE_DEPTH_M` below the
/// floor under each profile point (lower chain), then the top at `height_scale` × the
/// profile height, back to front (upper chain). `start_x` shifts it along the piece.
fn profile_section(
    profile: &[(f32, f32)],
    start_x: f32,
    z: f32,
    height_scale: f32,
) -> Vec<[f32; 3]> {
    let mut points = Vec::new();
    for (x, _) in profile {
        points.push([start_x + x, -HULL_BASE_DEPTH_M, z]);
    }
    for (x, height_m) in profile.iter().rev() {
        points.push([start_x + x, height_scale * height_m, z]);
    }
    points
}

/// How many segments a curve ending at `lip_angle_deg` needs so that no two neighbours
/// meet at more than `MAX_KINK_DEG`, and at least `fewest`.
fn segments_for(lip_angle_deg: f32, fewest: usize) -> usize {
    let needed = libm::ceilf(lip_angle_deg / MAX_KINK_DEG) as usize;
    needed.max(fewest)
}

/// A ramp's side profile as (distance along, height) points, in m: straight
/// segments (at least `RAMP_SEGMENTS`), each steeper than the last by the same step, ending at
/// `lip_angle_deg`. So a fast car meets no sharp kink (a straight wedge's foot crashed
/// the car above about 25 m/s), and the ramp ends lower than `length · tan(angle)`.
fn ramp_profile(length: f32, lip_angle_deg: f32) -> Vec<(f32, f32)> {
    let segments = segments_for(lip_angle_deg, RAMP_SEGMENTS);
    let segment_length_m = length / segments as f32;
    let mut profile = vec![(0.0, 0.0)];
    let mut height_m = 0.0;
    for segment in 1..=segments {
        let slope_deg = lip_angle_deg * segment as f32 / segments as f32;
        height_m += segment_length_m * libm::tanf(slope_deg.to_radians());
        profile.push((segment as f32 * segment_length_m, height_m));
    }
    profile
}

/// How high a ramp of `length` and `lip_angle_deg` is at its lip, in m (what the gap
/// pit after it must start at).
pub fn ramp_lip_height_m(length: f32, lip_angle_deg: f32) -> f32 {
    ramp_profile(length, lip_angle_deg)
        .last()
        .map_or(0.0, |point| point.1)
}

/// The dimensions of a wall's table-top ramp for a wall `height_m` tall: the curve's
/// length and lip angle (deg) for radius `MIN_CONCAVE_RADIUS_M`. On a parabola of radius
/// R rising h, the angle is sqrt(2h/R) and the length R · angle.
pub fn wall_ramp_dims(height_m: f32) -> (f32, f32) {
    let angle_rad = libm::sqrtf(2.0 * height_m / MIN_CONCAVE_RADIUS_M);
    (MIN_CONCAVE_RADIUS_M * angle_rad, angle_rad.to_degrees())
}

/// How long a wall with a ramp is along the course (ramp up, deck, ramp down), in m.
pub fn wall_ramp_total_length_m(height_m: f32) -> f32 {
    2.0 * wall_ramp_dims(height_m).0 + WALL_THICKNESS_M
}

/// The side profile of a wall's table top: the ramp, a flat deck and the ramp mirrored.
fn table_top_profile(height_m: f32) -> Vec<(f32, f32)> {
    let (length_m, angle_deg) = wall_ramp_dims(height_m);
    let up = ramp_profile(length_m, angle_deg);
    let deck_start_m = length_m;
    let mut profile = up.clone();
    let deck_height_m = up.last().map_or(0.0, |point| point.1);
    profile.push((deck_start_m + WALL_THICKNESS_M, deck_height_m));
    // The way down is the way up backwards: steepest at the deck, flat at the floor.
    for (x_m, y_m) in up.iter().rev().skip(1) {
        profile.push((deck_start_m + WALL_THICKNESS_M + (length_m - x_m), *y_m));
    }
    profile
}

/// A side profile: (distance along the piece, height) points, in m.
type SideProfile = Vec<(f32, f32)>;

/// A corkscrew's shape: `length` along the piece, `width` across, its low edge rising
/// to `lip_angle_deg` and its surface banked up to `bank_deg` at the lip.
struct CorkscrewTwist {
    length: f32,
    width: f32,
    lip_angle_deg: f32,
    bank_deg: f32,
}

impl CorkscrewTwist {
    /// The two edges' side profiles as (distance along, height) points, in m: the low
    /// edge, then the raised edge. Both start flat at the floor. The low edge curves up
    /// to `lip_angle_deg` (h = L·tan(angle)·s²/2 with s = x/L, so its slope grows evenly
    /// from 0), and the bank grows as bank·s², so the surface twists smoothly from flat
    /// at the foot to steepest at the lip, and the raised edge sits width·tan(bank)
    /// above the low edge.
    fn edge_profiles(&self) -> (SideProfile, SideProfile) {
        let lip_slope = libm::tanf(self.lip_angle_deg.to_radians());
        let samples = segments_for(self.lip_angle_deg, CORKSCREW_SAMPLES);
        let mut low_edge = Vec::new();
        let mut raised_edge = Vec::new();
        for sample in 0..=samples {
            let progress = sample as f32 / samples as f32;
            let x = progress * self.length;
            let low_m = self.length * lip_slope * progress * progress / 2.0;
            let bank_rad = (self.bank_deg * progress * progress).to_radians();
            let raised_m = low_m + self.width * libm::tanf(bank_rad);
            low_edge.push((x, low_m));
            raised_edge.push((x, raised_m));
        }
        (low_edge, raised_edge)
    }
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
    /// sheer back (see `ramp_profile`): one smooth mesh.
    fn ramp(
        &self,
        surface: Surface,
        start_x: f32,
        offset_z: f32,
        length: f32,
        width: f32,
        lip_angle_deg: f32,
    ) -> Vec<CourseShape> {
        let profile = ramp_profile(length, lip_angle_deg);
        let section = |z: f32| profile_section(&profile, start_x, z, 1.0);
        let near = section(offset_z - width / 2.0);
        let far = section(offset_z + width / 2.0);
        vec![self.mesh(surface, loft(&near, &far))]
    }

    /// A kicker: a ramp beside the lane's centre line (z = 0) out to `outer_z`, whose
    /// top also slopes across, from 0 at the centre line to the full ramp height at
    /// `outer_z`. So the wheels on that side ride up and the car rolls, with no wall
    /// under the car for the chassis to jam against.
    fn kicker(&self, length: f32, outer_z: f32, lip_angle_deg: f32) -> Vec<CourseShape> {
        let profile = ramp_profile(length, lip_angle_deg);
        let centre_line = profile_section(&profile, 0.0, 0.0, 0.0);
        let outer = profile_section(&profile, 0.0, outer_z, 1.0);
        vec![self.mesh(Surface::Ramp, loft(&centre_line, &outer))]
    }

    /// A corkscrew centred on the lane (see `CorkscrewTwist`), its raised edge on the
    /// side of `raised_side_z` (−1 left, +1 right): one smooth mesh lofted across the
    /// lane between the low edge and the raised edge.
    fn corkscrew(&self, twist: &CorkscrewTwist, raised_side_z: f32) -> Vec<CourseShape> {
        let (low_profile, raised_profile) = twist.edge_profiles();
        let half_width = twist.width / 2.0;
        let low_edge = profile_section(&low_profile, 0.0, -raised_side_z * half_width, 1.0);
        let raised_edge = profile_section(&raised_profile, 0.0, raised_side_z * half_width, 1.0);
        vec![self.mesh(Surface::Ramp, loft(&low_edge, &raised_edge))]
    }

    /// A flat non-solid marking centred on the lane, `length` long from the piece start
    /// and `width` across, in m.
    fn marking(&self, surface: Surface, length: f32, width: f32) -> CourseShape {
        let half = [length / 2.0, MARKING_THICKNESS_M / 2.0, width / 2.0];
        let mut shape = self.local_box(surface, [half[0], half[1], 0.0], half, 0.0);
        shape.solid = false;
        shape
    }

    /// A closed mesh given in the piece frame.
    fn mesh(&self, surface: Surface, mesh: LoftedMesh) -> CourseShape {
        let geometry = ShapeGeometry::Mesh {
            points_m: mesh.points_m,
            triangles: mesh.triangles,
        };
        self.shape(surface, geometry, Vector::ZERO, Rotation::IDENTITY)
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
            friction: None,
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
        friction: None,
    }
}
