//! The landing forecast (plan 2026-10-27 G4): where and when the airborne car will touch
//! down, what the surface there looks like, and how far the answer can be trusted.
//!
//! The path is ballistic (gravity only: no drag, wind or thrust), sampled every
//! `STEP_S`. Touchdown is the first sample whose centre of mass is within `rest_height_m`
//! of the ground straight below it. The same path is flown again with the speed scaled by
//! ±`speed_spread` and the pitch of the velocity tilted by ±`pitch_spread_deg`; the spread
//! of those touchdowns is the uncertainty.

use rapier3d::prelude::*;

use crate::car::Car;
use crate::course_build::OBSTACLE_USER_DATA;

/// Spacing of the sampled path, in s.
const STEP_S: f32 = 1.0 / 60.0;
/// Spacing of the ground samples used for the usable length, in m.
const GROUND_STEP_M: f32 = 0.5;
/// How far a ground ray reaches down from just above the surface, in m.
const GROUND_REACH_M: f32 = 12.0;
/// How far above the touchdown point the ground samples start, in m.
const GROUND_LIFT_M: f32 = 4.0;
/// How far a touchdown ray is lifted when its start fell inside the ground, in m.
const INSIDE_LIFT_M: f32 = 2.0;

/// What the forecast assumes (all but `max_time_s` are `[stunt]` entries in
/// `configs/controllers.toml`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForecastSettings {
    /// Relative speed spread of the uncertainty samples (0.02 = ±2%).
    pub speed_spread: f32,
    /// Pitch spread of the velocity in the uncertainty samples, in degrees.
    pub pitch_spread_deg: f32,
    /// A surface steeper than this from level is not flat enough to count as usable, in
    /// degrees.
    pub flat_tilt_deg: f32,
    /// How high the centre of mass sits when the wheels rest on the ground, in m.
    pub rest_height_m: f32,
    /// Give up if the car is still in the air after this long, in s.
    pub max_time_s: f32,
    /// The usable length is measured out to this distance ahead of the point, in m.
    pub max_length_ahead_m: f32,
    /// ... and this far behind it, in m.
    pub max_length_behind_m: f32,
}

impl Default for ForecastSettings {
    fn default() -> ForecastSettings {
        ForecastSettings {
            speed_spread: 0.02,
            pitch_spread_deg: 1.0,
            flat_tilt_deg: 15.0,
            rest_height_m: 0.6,
            max_time_s: 8.0,
            max_length_ahead_m: 30.0,
            max_length_behind_m: 8.0,
        }
    }
}

/// Where and when the car will land.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandingForecast {
    /// Seconds from now until touchdown on the nominal path.
    pub time_s: f32,
    /// The ground point under the car at touchdown, in world metres.
    pub point_m: [f32; 3],
    /// Unit surface normal at that point.
    pub normal: [f32; 3],
    /// Angle between the normal and straight up, in degrees.
    pub normal_tilt_deg: f32,
    /// Length of flat-enough, obstacle-free surface around the point, along the direction
    /// of travel, in m. 0 if the point itself is not usable.
    pub usable_length_m: f32,
    /// Largest touchdown-time difference between the nominal path and a sample, in s.
    pub time_uncertainty_s: f32,
    /// Largest horizontal touchdown-position difference, in m.
    pub position_uncertainty_m: f32,
}

/// One flown path's touchdown.
struct Touchdown {
    time_s: f32,
    point: Vector,
    normal: Vector,
    /// The surface hit is an obstacle (a parked car, a wall), not the floor or a ramp.
    on_obstacle: bool,
}

/// Spacing of the cheap landing view's path samples, in s (twice `STEP_S`: it runs every
/// control step while airborne).
const VIEW_STEP_S: f32 = 1.0 / 30.0;
/// The landing view gives up after this long in the air, in s.
const VIEW_MAX_TIME_S: f32 = 12.0;
/// Edge probes run out to this distance from the touchdown point, in m, in steps of
/// `GROUND_STEP_M`; an edge farther than this is "none".
pub const EDGE_FAR_M: f32 = 6.0;
/// Between two edge probes the surface may change height by this much and still count
/// as the same surface, in m (a 40 degree slope over one step).
const EDGE_MAX_RISE_M: f32 = 0.42;

/// The cheap landing summary a controller gets every control step while airborne: the
/// nominal path only (no uncertainty samples), flown with a coarser step than the stunt
/// forecast.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandingView {
    /// Seconds until touchdown on the ballistic path; infinite if nothing is hit in time.
    pub time_s: f32,
    /// How far the centre of mass is above the surface point it will come down on, in m;
    /// infinite with no touchdown.
    pub gap_m: f32,
    /// Unit surface normal there, in the body frame (up when there is no touchdown).
    pub normal_body: [f32; 3],
    /// The touchdown is on an obstacle top, so a nose or tail can overhang an edge.
    pub on_obstacle: bool,
    /// Distance from the touchdown point to where the surface ends or steps away, towards
    /// the car's front, back, right and left, in m, at most `EDGE_FAR_M`.
    pub edge_m: [f32; 4],
    /// The mode has a limited tank (the world fills this in; the forecast does not know).
    pub limited_air: bool,
    /// Air left in the tank, in kg.
    pub air_kg: f32,
}

/// Forecasts the landing of a body at `position` with `velocity`, or None if it does not
/// touch anything within `settings.max_time_s`.
pub(crate) fn forecast(
    physics: &PhysicsWorld,
    car: &Car,
    position: Vector,
    velocity: Vector,
    settings: &ForecastSettings,
) -> Option<LandingForecast> {
    let gravity = physics.gravity;
    let nominal = touch_down(physics, car, position, velocity, gravity, settings, STEP_S)?;
    let mut time_spread = 0.0_f32;
    let mut position_spread = 0.0_f32;
    for speed_scale in [
        1.0 - settings.speed_spread,
        1.0,
        1.0 + settings.speed_spread,
    ] {
        for pitch_deg in [-settings.pitch_spread_deg, 0.0, settings.pitch_spread_deg] {
            if speed_scale == 1.0 && pitch_deg == 0.0 {
                continue;
            }
            let sample_velocity = tilt_pitch(velocity * speed_scale, pitch_deg.to_radians());
            match touch_down(
                physics,
                car,
                position,
                sample_velocity,
                gravity,
                settings,
                STEP_S,
            ) {
                Some(sample) => {
                    time_spread = time_spread.max((sample.time_s - nominal.time_s).abs());
                    let offset = sample.point - nominal.point;
                    position_spread =
                        position_spread.max(Vector::new(offset.x, 0.0, offset.z).length());
                }
                // A sample that never lands within the horizon: as uncertain as it gets.
                None => {
                    time_spread = time_spread.max(settings.max_time_s - nominal.time_s);
                    position_spread = position_spread.max(velocity.length() * settings.max_time_s);
                }
            }
        }
    }
    let heading = horizontal(velocity).unwrap_or_else(|| {
        horizontal(*physics.bodies[car.chassis].rotation() * Vector::X).unwrap_or(Vector::X)
    });
    let usable_length_m = usable_length(physics, car, &nominal, heading, settings);
    let normal_tilt_deg = nominal.normal.y.clamp(-1.0, 1.0).acos().to_degrees();
    Some(LandingForecast {
        time_s: nominal.time_s,
        point_m: nominal.point.to_array(),
        normal: nominal.normal.to_array(),
        normal_tilt_deg,
        usable_length_m,
        time_uncertainty_s: time_spread,
        position_uncertainty_m: position_spread,
    })
}

/// Flies one ballistic path until the centre of mass is within the rest height of the
/// ground below it.
fn touch_down(
    physics: &PhysicsWorld,
    car: &Car,
    start: Vector,
    velocity: Vector,
    gravity: Vector,
    settings: &ForecastSettings,
    step_s: f32,
) -> Option<Touchdown> {
    let filter = QueryFilter::default().exclude_rigid_body(car.chassis);
    let down = Vector::new(0.0, -1.0, 0.0);
    let steps = (settings.max_time_s / step_s).ceil() as u32;
    let reach = settings.rest_height_m + 60.0;
    let mut previous_gap: Option<f32> = None;
    for step in 0..=steps {
        let t = step as f32 * step_s;
        let position = start + velocity * t + gravity * (0.5 * t * t);
        let ray = Ray::new(position, down);
        let mut cast = physics.cast_ray_and_get_normal(&ray, reach, true, filter);
        if let Some((_, hit)) = &cast {
            // A fast fall can step from just above the ground to just inside it, and a ray
            // that starts inside a solid reports distance 0 and no normal. Cast again from
            // a little higher, so the touchdown still gets its surface and normal.
            if hit.time_of_impact <= 0.0 {
                let lift = INSIDE_LIFT_M;
                let higher = Ray::new(position + Vector::new(0.0, lift, 0.0), down);
                cast = physics.cast_ray_and_get_normal(&higher, reach + lift, true, filter);
                if let Some((_, lifted)) = cast.as_mut() {
                    lifted.time_of_impact -= lift;
                }
            }
        }
        if let Some((handle, hit)) = cast {
            let gap = hit.time_of_impact - settings.rest_height_m;
            if gap <= 0.0 {
                // Interpolate where the gap closed between the last sample and this one.
                let time_s = match previous_gap {
                    Some(before) if before > 0.0 => t - step_s * (-gap) / (before - gap),
                    _ => t,
                };
                let point = position + down * hit.time_of_impact;
                return Some(Touchdown {
                    time_s: time_s.max(0.0),
                    point,
                    normal: hit.normal,
                    on_obstacle: physics.colliders[handle].user_data == OBSTACLE_USER_DATA,
                });
            }
            previous_gap = Some(gap);
        } else {
            previous_gap = None;
        }
    }
    None
}

/// Tilts `velocity`'s pitch by `angle_rad` about the horizontal axis across its heading.
fn tilt_pitch(velocity: Vector, angle_rad: f32) -> Vector {
    let flat = Vector::new(velocity.x, 0.0, velocity.z);
    let axis = Vector::Y.cross(flat);
    if axis.length() < 1e-6 {
        return velocity;
    }
    Rotation::from_axis_angle(axis.normalize(), angle_rad) * velocity
}

/// The heading of `v` on the ground plane as a unit vector; None when it is nearly vertical.
fn horizontal(v: Vector) -> Option<Vector> {
    let flat = Vector::new(v.x, 0.0, v.z);
    (flat.length() > 0.5).then(|| flat.normalize())
}

/// How far the ground around the touchdown point is flat enough and free of obstacles,
/// ahead of it and behind it, along `heading`.
fn usable_length(
    physics: &PhysicsWorld,
    car: &Car,
    touchdown: &Touchdown,
    heading: Vector,
    settings: &ForecastSettings,
) -> f32 {
    let filter = QueryFilter::default().exclude_rigid_body(car.chassis);
    let max_rise = GROUND_STEP_M * settings.flat_tilt_deg.to_radians().tan() + 0.02;
    let min_normal_y = settings.flat_tilt_deg.to_radians().cos();
    // None = the surface at this offset is not usable; Some(height) = it is.
    let probe = |offset_m: f32| -> Option<f32> {
        let from = touchdown.point + heading * offset_m + Vector::new(0.0, GROUND_LIFT_M, 0.0);
        let (handle, hit) = physics.cast_ray_and_get_normal(
            &Ray::new(from, Vector::new(0.0, -1.0, 0.0)),
            GROUND_REACH_M,
            true,
            filter,
        )?;
        let obstacle = physics.colliders[handle].user_data == OBSTACLE_USER_DATA;
        (!obstacle && hit.normal.y >= min_normal_y).then_some(from.y - hit.time_of_impact)
    };
    let Some(point_height) = probe(0.0) else {
        return 0.0;
    };
    let extent = |direction: f32, limit_m: f32| -> f32 {
        let mut last = point_height;
        let mut length = 0.0;
        while length + GROUND_STEP_M <= limit_m {
            match probe(direction * (length + GROUND_STEP_M)) {
                Some(height) if (height - last).abs() <= max_rise => {
                    last = height;
                    length += GROUND_STEP_M;
                }
                _ => break,
            }
        }
        length
    };
    extent(1.0, settings.max_length_ahead_m) + extent(-1.0, settings.max_length_behind_m)
}

/// The landing view for a body at `position` with `velocity`: the nominal touchdown, its
/// surface normal (turned into the body frame by `to_body`), whether it is an obstacle
/// top, and the edge distances along the body's `front` and `right` axes. Always Some;
/// with no touchdown in `VIEW_MAX_TIME_S` the time and gap are infinite.
pub(crate) fn landing_view(
    physics: &PhysicsWorld,
    car: &Car,
    position: Vector,
    velocity: Vector,
    to_body: impl Fn(Vector) -> [f32; 3],
) -> LandingView {
    let settings = ForecastSettings {
        max_time_s: VIEW_MAX_TIME_S,
        ..ForecastSettings::default()
    };
    let touchdown = touch_down(
        physics,
        car,
        position,
        velocity,
        physics.gravity,
        &settings,
        VIEW_STEP_S,
    );
    let Some(touchdown) = touchdown else {
        return LandingView {
            time_s: f32::INFINITY,
            gap_m: f32::INFINITY,
            normal_body: to_body(Vector::Y),
            limited_air: false,
            air_kg: 0.0,
            on_obstacle: false,
            edge_m: [EDGE_FAR_M; 4],
        };
    };
    let rotation = *physics.bodies[car.chassis].rotation();
    let front = horizontal(rotation * Vector::X).unwrap_or(Vector::X);
    let right = horizontal(rotation * Vector::Z).unwrap_or(Vector::Z);
    let edge_m = [front, -front, right, -right]
        .map(|direction| edge_distance(physics, car, &touchdown, direction));
    LandingView {
        time_s: touchdown.time_s,
        gap_m: position.y - touchdown.point.y,
        normal_body: to_body(touchdown.normal),
        limited_air: false,
        air_kg: 0.0,
        on_obstacle: touchdown.on_obstacle,
        edge_m,
    }
}

/// How far from the touchdown point along `direction` the surface stays at about the same
/// height (no step down, no wall), in m, at most `EDGE_FAR_M`.
fn edge_distance(
    physics: &PhysicsWorld,
    car: &Car,
    touchdown: &Touchdown,
    direction: Vector,
) -> f32 {
    let filter = QueryFilter::default().exclude_rigid_body(car.chassis);
    let height_at = |offset_m: f32| -> Option<f32> {
        let from = touchdown.point + direction * offset_m + Vector::new(0.0, GROUND_LIFT_M, 0.0);
        let ray = Ray::new(from, Vector::new(0.0, -1.0, 0.0));
        let (_, hit) = physics.cast_ray_and_get_normal(&ray, GROUND_REACH_M, true, filter)?;
        Some(from.y - hit.time_of_impact)
    };
    let mut last = touchdown.point.y;
    let mut length = 0.0;
    while length + GROUND_STEP_M <= EDGE_FAR_M {
        match height_at(length + GROUND_STEP_M) {
            Some(height) if (height - last).abs() <= EDGE_MAX_RISE_M => {
                last = height;
                length += GROUND_STEP_M;
            }
            _ => break,
        }
    }
    length
}
