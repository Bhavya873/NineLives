//! The driver bot (SPEC §9.7, training only): drives Stages C, D and E like a slightly
//! sloppy person. Pure-pursuit steering on the course centre line, P control on speed,
//! random steering wobble and jerks, and a Hop press before each car row (or at a set
//! time on Stage C). It never touches the manual thrusters: the controller does all the
//! thrusting. It sees only a `BotView` of the car and draws its randomness from the
//! episode seed on ChaCha8 stream 5, so the same seed always drives the same way.

use rand::RngExt;
use rand_chacha::ChaCha8Rng;
use serde::Deserialize;

use crate::car::steering_limit_rad;
use crate::conventions::PHYSICS_DT_S;
use crate::course::{CourseSpec, PieceSpec};
use crate::course_build::car_row_length_m;
use crate::driving_stages::stream_rng;
use crate::hop_profile::HopProfile;
use crate::input::{BUTTON_HOP, InputFrame};
use crate::randomization::gaussian;
use crate::vehicle_config::{SteeringConfig, VehicleConfig};
use crate::world::World;

/// The ChaCha8 stream for the bot's randomness.
const BOT_RNG_STREAM: u64 = 5;
/// Largest value of an `InputFrame` axis.
const INPUT_FULL_SCALE: f32 = 127.0;

/// The bot's settings (`[driver_bot]` in scenarios.toml, SPEC §9.7).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverBotConfig {
    /// Target speed range on Stage D, [low, high], in m/s.
    pub target_speed_m_s: [f32; 2],
    /// Pure-pursuit look-ahead: this many metres…
    pub lookahead_base_m: f32,
    /// …plus this many seconds of travel at the current speed, in s.
    pub lookahead_per_speed_s: f32,
    /// The pursuit never asks for a turn pulling harder sideways than this, in m/s².
    pub max_turn_accel_m_s2: f32,
    /// Throttle per m/s of speed error (P control).
    pub speed_gain_per_m_s: f32,
    /// Steady-state size (standard deviation) of the OU steering wobble, as a fraction of
    /// the grip-safe turn.
    pub steer_noise_sigma: f32,
    /// How fast the wobble returns to zero, per s.
    pub steer_noise_return_per_s: f32,
    /// Chance per second of a sudden steering jerk.
    pub jerk_chance_per_s: f32,
    /// Size of a jerk, as a fraction of the grip-safe turn (either way).
    pub jerk_size: f32,
    /// How long a jerk lasts, in s.
    pub jerk_s: f32,
    /// The Hop press comes up to this early or late, in s.
    pub hop_timing_noise_s: f32,
    /// The wheels should pass this far above a car row, in m (for Hop timing).
    pub hop_clearance_m: f32,
}

/// What the bot sees of the car on one physics step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BotView {
    /// Centre position in world coordinates, in m.
    pub position_m: [f32; 3],
    /// The nose direction in world coordinates (unit vector).
    pub forward: [f32; 3],
    /// The car's right side (body +Z) in world coordinates (unit vector).
    pub right: [f32; 3],
    /// Speed along the nose, in m/s.
    pub speed_m_s: f32,
    /// A Hop is pushing now.
    pub hop_active: bool,
    /// Any wheel touches the ground.
    pub on_wheels: bool,
}

impl BotView {
    /// What the bot sees of `world`'s car now.
    pub fn of(world: &World) -> BotView {
        let rotation = world.chassis_pose().to_rapier().rotation;
        let forward = rotation * rapier3d::prelude::Vector::X;
        let right = rotation * rapier3d::prelude::Vector::Z;
        BotView {
            position_m: world.chassis_pose().position_m,
            forward: forward.to_array(),
            right: right.to_array(),
            speed_m_s: world.forward_speed_m_s(),
            hop_active: world.hop_boost().hop_active(),
            on_wheels: world.wheels_touching_count() > 0,
        }
    }
}

/// The line the bot follows: from `origin` along `direction`, both on the ground (x, z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Centerline {
    /// A point on the line (world x, z), in m.
    pub origin_m: [f32; 2],
    /// The direction the course runs (world x, z), a unit vector.
    pub direction: [f32; 2],
}

impl Centerline {
    /// World +X through the origin (flat ground's "course").
    pub fn along_x() -> Centerline {
        Centerline {
            origin_m: [0.0, 0.0],
            direction: [1.0, 0.0],
        }
    }

    /// The course's centre line: from its start, along its start heading (courses run
    /// straight, SPEC §7.11).
    pub fn of_course(course: &CourseSpec) -> Centerline {
        let yaw_rad = course.start.yaw.to_radians();
        // Yaw turns about +Y, so a positive yaw swings +X towards −Z (SPEC §4).
        Centerline {
            origin_m: [course.start.pos[0], course.start.pos[2]],
            direction: [libm::cosf(yaw_rad), -libm::sinf(yaw_rad)],
        }
    }

    /// How far along the line `position_m` (world) is, in m.
    pub fn along_m(&self, position_m: [f32; 3]) -> f32 {
        let dx = position_m[0] - self.origin_m[0];
        let dz = position_m[2] - self.origin_m[1];
        dx * self.direction[0] + dz * self.direction[1]
    }

    /// How far to the right of the line `position_m` (world) is, in m (right is +Z when
    /// the course runs along +X, SPEC §4).
    pub fn right_of_line_m(&self, position_m: [f32; 3]) -> f32 {
        let dx = position_m[0] - self.origin_m[0];
        let dz = position_m[2] - self.origin_m[1];
        // The line's right-hand direction is (−dz, dx) for a direction (dx, dz).
        -dx * self.direction[1] + dz * self.direction[0]
    }

    /// The point on the line `along_m` from its origin, in world coordinates (y = 0).
    pub fn point_m(&self, along_m: f32) -> [f32; 3] {
        [
            self.origin_m[0] + self.direction[0] * along_m,
            0.0,
            self.origin_m[1] + self.direction[1] * along_m,
        ]
    }
}

/// When the bot presses Hop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HopTrigger {
    /// At this time after the start, in s.
    AtTime { time_s: f32 },
    /// Before a car row: when the row's middle is `lead_s` of travel ahead, so the hop
    /// is highest over it. Gives up once the car is past `end_m` along the course.
    BeforeRow {
        middle_m: f32,
        end_m: f32,
        lead_s: f32,
    },
}

/// One planned Hop and how far it has got.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PlannedHop {
    trigger: HopTrigger,
    /// This hop's random timing offset, in s (positive presses earlier).
    timing_noise_s: f32,
    /// The trigger has fired and the bot is pressing until a Hop starts.
    pressing: bool,
    done: bool,
}

/// A running driver bot.
#[derive(Debug, Clone)]
pub struct DriverBot {
    config: DriverBotConfig,
    steering: SteeringConfig,
    wheelbase_m: f32,
    centerline: Centerline,
    target_speed_m_s: f32,
    hops: Vec<PlannedHop>,
    rng: ChaCha8Rng,
    /// The OU wobble added to the steering, in steer units.
    steer_noise: f32,
    /// The jerk now being applied (steer units) and how long it has left (s).
    jerk: Option<(f32, f32)>,
    /// Hop was down on the last step (a press needs it up first).
    hop_was_down: bool,
    /// Physics steps since the start (counted, not summed, so times are exact).
    steps: u32,
}

impl DriverBot {
    /// A bot following `centerline` at `target_speed_m_s`, pressing Hop at `triggers`.
    /// `seed` is the episode seed.
    pub fn new(
        config: DriverBotConfig,
        vehicle: &VehicleConfig,
        centerline: Centerline,
        target_speed_m_s: f32,
        triggers: &[HopTrigger],
        seed: u64,
    ) -> DriverBot {
        let mut rng = stream_rng(seed, BOT_RNG_STREAM);
        let noise_s = config.hop_timing_noise_s;
        let mut hops = Vec::new();
        for trigger in triggers {
            hops.push(PlannedHop {
                trigger: *trigger,
                timing_noise_s: rng.random_range(-noise_s..=noise_s),
                pressing: false,
                done: false,
            });
        }
        let mounts = &vehicle.wheels.mount_points_m;
        DriverBot {
            config,
            steering: vehicle.steering.clone(),
            wheelbase_m: mounts[0][0] - mounts[2][0],
            centerline,
            target_speed_m_s,
            hops,
            rng,
            steer_noise: 0.0,
            jerk: None,
            hop_was_down: false,
            steps: 0,
        }
    }

    /// The speed the bot holds, in m/s.
    pub fn target_speed_m_s(&self) -> f32 {
        self.target_speed_m_s
    }

    /// The driver's input for this physics step. The wobble and jerks are fractions of
    /// the grip-safe turn (`safe_turn_rad`), so they are the same size at every speed.
    /// In the air it centres the steering, as a person would: wheels turned while flying
    /// land the car in a sudden swerve.
    pub fn input(&mut self, view: &BotView) -> InputFrame {
        let wobble = self.next_wobble();
        let speed_m_s = view.speed_m_s.max(0.0);
        let mut steer = 0.0;
        if view.on_wheels {
            let wheel_rad = self.pursuit_wheel_rad(view) + wobble * self.safe_turn_rad(speed_m_s);
            steer = wheel_rad / steering_limit_rad(&self.steering, speed_m_s).max(1e-3);
        }
        let throttle = self.config.speed_gain_per_m_s * (self.target_speed_m_s - view.speed_m_s);
        let hop_down = self.hop_button(view);
        self.hop_was_down = hop_down;
        self.steps += 1;
        InputFrame {
            throttle: to_axis(throttle),
            steer: to_axis(steer),
            buttons: if hop_down { BUTTON_HOP } else { 0 },
        }
    }

    /// The pursuit's steering as an input fraction, −1..1 (positive steers left).
    pub fn pursuit_steer(&self, view: &BotView) -> f32 {
        let speed_m_s = view.speed_m_s.max(0.0);
        let limit_rad = steering_limit_rad(&self.steering, speed_m_s);
        (self.pursuit_wheel_rad(view) / limit_rad.max(1e-3)).clamp(-1.0, 1.0)
    }

    /// Pure pursuit (SPEC §9.7): the front-wheel angle that puts the car on the arc
    /// through the centre-line point one look-ahead distance ahead, no sharper than
    /// `safe_turn_rad`. Positive turns left, in rad.
    fn pursuit_wheel_rad(&self, view: &BotView) -> f32 {
        let speed_m_s = view.speed_m_s.max(0.0);
        let lookahead_m =
            self.config.lookahead_base_m + self.config.lookahead_per_speed_s * speed_m_s;
        let along_m = self.centerline.along_m(view.position_m);
        let target_m = self.centerline.point_m(along_m + lookahead_m);
        let to_target = [
            target_m[0] - view.position_m[0],
            target_m[2] - view.position_m[2],
        ];
        let ahead_m = to_target[0] * view.forward[0] + to_target[1] * view.forward[2];
        let right_m = to_target[0] * view.right[0] + to_target[1] * view.right[2];
        let distance_m = (ahead_m * ahead_m + right_m * right_m).sqrt().max(1e-3);
        // The target's angle off the nose; positive when it is to the left (−Z).
        let alpha_rad = libm::atan2f(-right_m, ahead_m);
        let wheel_rad = libm::atanf(2.0 * self.wheelbase_m * libm::sinf(alpha_rad) / distance_m);
        let safe_rad = self.safe_turn_rad(speed_m_s);
        wheel_rad.clamp(-safe_rad, safe_rad)
    }

    /// The sharpest wheel angle whose turn the tyres hold at `speed_m_s`: a wheel angle
    /// δ at speed v pulls about v²·tan δ / L sideways, kept to `max_turn_accel_m_s2`, so
    /// the car does not slide and spin. In rad.
    fn safe_turn_rad(&self, speed_m_s: f32) -> f32 {
        let pull = self.config.max_turn_accel_m_s2 * self.wheelbase_m;
        let safe_rad = libm::atanf(pull / (speed_m_s * speed_m_s).max(1e-3));
        safe_rad.min(steering_limit_rad(&self.steering, speed_m_s))
    }

    /// The steering wobble for this step: an Ornstein–Uhlenbeck process plus the
    /// occasional jerk (SPEC §9.7).
    fn next_wobble(&mut self) -> f32 {
        let config = self.config;
        let theta = config.steer_noise_return_per_s;
        // Scaled so the wobble's steady-state standard deviation is `steer_noise_sigma`.
        let kick = config.steer_noise_sigma * (2.0 * theta * PHYSICS_DT_S).sqrt();
        self.steer_noise +=
            -theta * self.steer_noise * PHYSICS_DT_S + kick * gaussian(&mut self.rng);
        self.jerk = match self.jerk {
            Some((size, left_s)) if left_s > PHYSICS_DT_S => Some((size, left_s - PHYSICS_DT_S)),
            Some(_) => None,
            None => self.maybe_start_jerk(),
        };
        let jerk = self.jerk.map_or(0.0, |(size, _)| size);
        self.steer_noise + jerk
    }

    /// With a `jerk_chance_per_s` chance per second, a jerk of ±`jerk_size`.
    fn maybe_start_jerk(&mut self) -> Option<(f32, f32)> {
        let chance = self.config.jerk_chance_per_s * PHYSICS_DT_S;
        if self.rng.random_range(0.0..1.0) >= chance {
            return None;
        }
        let side = if self.rng.random_range(0.0..1.0) < 0.5 {
            1.0
        } else {
            -1.0
        };
        Some((side * self.config.jerk_size, self.config.jerk_s))
    }

    /// Whether Hop is held down this step. Once a hop's trigger fires, the bot taps the
    /// button (down one step, up the next) until a Hop starts, since a Hop can be
    /// blocked for a moment (wheels up, cooldown).
    fn hop_button(&mut self, view: &BotView) -> bool {
        let along_m = self.centerline.along_m(view.position_m);
        let elapsed_s = self.steps as f32 * PHYSICS_DT_S;
        let Some(hop) = self.hops.iter_mut().find(|hop| !hop.done) else {
            return false;
        };
        if hop.pressing && view.hop_active {
            hop.done = true;
            return false;
        }
        if hop.trigger.is_too_late(along_m) {
            hop.done = true;
            return false;
        }
        if !hop.pressing {
            hop.pressing =
                hop.trigger
                    .fires(along_m, view.speed_m_s, elapsed_s, hop.timing_noise_s);
        }
        hop.pressing && !self.hop_was_down
    }
}

impl HopTrigger {
    /// True once the press should come.
    fn fires(&self, along_m: f32, speed_m_s: f32, elapsed_s: f32, noise_s: f32) -> bool {
        match *self {
            HopTrigger::AtTime { time_s } => elapsed_s >= time_s - noise_s,
            HopTrigger::BeforeRow {
                middle_m, lead_s, ..
            } => middle_m - along_m <= speed_m_s.max(0.0) * (lead_s + noise_s),
        }
    }

    /// True once pressing would no longer help.
    fn is_too_late(&self, along_m: f32) -> bool {
        match *self {
            HopTrigger::AtTime { .. } => false,
            HopTrigger::BeforeRow { end_m, .. } => along_m > end_m,
        }
    }
}

/// True if `course` has a car row marked `hop_hint` (so the bot needs a hop profile).
pub fn has_hop_rows(course: &CourseSpec) -> bool {
    course
        .pieces
        .iter()
        .any(|piece| matches!(piece, PieceSpec::CarRow { hop_hint: true, .. }))
}

/// A Hop trigger before every car row of `course` marked `hop_hint`, timed with this
/// car's hop `profile` so the hop is highest over the row's middle (SPEC §9.7). Sorted
/// by position along the track, the order the car meets them (file order may differ).
pub fn row_hop_triggers(
    course: &CourseSpec,
    centerline: &Centerline,
    profile: &HopProfile,
    clearance_m: f32,
) -> Vec<HopTrigger> {
    let mut rows: Vec<(f32, f32, f32)> = Vec::new();
    for piece in &course.pieces {
        if let PieceSpec::CarRow {
            pos,
            count,
            height,
            hop_hint: true,
            ..
        } = piece
        {
            let start_m = centerline.along_m(*pos);
            let length_m = car_row_length_m(*count);
            rows.push((start_m + length_m / 2.0, start_m + length_m, *height));
        }
    }
    rows.sort_by(|first, second| first.0.total_cmp(&second.0));
    let mut triggers = Vec::new();
    for (middle_m, end_m, height_m) in rows {
        triggers.push(HopTrigger::BeforeRow {
            middle_m,
            end_m,
            lead_s: profile.lead_time_s(height_m + clearance_m),
        });
    }
    triggers
}

/// A −1..1 value as an `InputFrame` axis.
fn to_axis(value: f32) -> i8 {
    (value.clamp(-1.0, 1.0) * INPUT_FULL_SCALE).round() as i8
}
