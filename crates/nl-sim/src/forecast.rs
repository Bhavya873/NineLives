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
    let nominal = touch_down(physics, car, position, velocity, gravity, settings)?;
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
            match touch_down(physics, car, position, sample_velocity, gravity, settings) {
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
) -> Option<Touchdown> {
    let filter = QueryFilter::default().exclude_rigid_body(car.chassis);
    let down = Vector::new(0.0, -1.0, 0.0);
    let steps = (settings.max_time_s / STEP_S).ceil() as u32;
    let reach = settings.rest_height_m + 60.0;
    let mut previous_gap: Option<f32> = None;
    for step in 0..=steps {
        let t = step as f32 * STEP_S;
        let position = start + velocity * t + gravity * (0.5 * t * t);
        let ray = Ray::new(position, down);
        if let Some((_, hit)) = physics.cast_ray_and_get_normal(&ray, reach, true, filter) {
            let gap = hit.time_of_impact - settings.rest_height_m;
            if gap <= 0.0 {
                // Interpolate where the gap closed between the last sample and this one.
                let time_s = match previous_gap {
                    Some(before) if before > 0.0 => t - STEP_S * (-gap) / (before - gap),
                    _ => t,
                };
                let point = position + down * hit.time_of_impact;
                return Some(Touchdown {
                    time_s: time_s.max(0.0),
                    point,
                    normal: hit.normal,
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
