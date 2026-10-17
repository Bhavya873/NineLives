//! The observation the AI sees (SPEC §9.2): 41 numbers in a fixed order, defined here
//! and nowhere else. Python, the policy exporter and the browser read this layout
//! through `OBS_SPEC` and check `obs_schema_hash` before trusting a policy
//! (CLAUDE.md golden rule 4). Any change to the names, their order or their ranges
//! changes the hash.

use crate::car_state::{CarState, MAX_HEIGHT_M, TERRAIN_PROBE_CAP_M};
use crate::hash::{FNV_OFFSET_BASIS, fnv1a_add_bytes};
use crate::world::World;

/// How many numbers an observation holds (SPEC §9.2, v1).
pub const OBS_DIM: usize = 41;

/// One observation: `OBS_DIM` numbers in `OBS_SPEC` order.
pub type Observation = [f32; OBS_DIM];

/// Largest spin the observation reports per axis, in rad/s; faster is clamped.
const MAX_SPIN_RAD_S: f32 = 30.0;
/// Largest speed the observation reports per axis, in m/s (the car's cap is 45 m/s).
const MAX_SPEED_M_S: f32 = 60.0;
/// Highest `thrust_scale` the observation expects: Arcade's 2.0 times the largest
/// domain-randomized force factor (1.15, SPEC §9.8), rounded up.
const MAX_THRUST_SCALE: f32 = 3.0;
/// Longest Hop push the observation expects, in s (Realistic's is 0.4 s).
const MAX_HOP_TIME_S: f32 = 1.0;

/// One number in the observation: its name and the range it is clamped to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObsField {
    /// Unique name, e.g. "ang_vel_body.x". The schema hash is built from these.
    pub name: &'static str,
    /// Lowest value it can take.
    pub low: f32,
    /// Highest value it can take.
    pub high: f32,
}

/// A field from its name and range (keeps the table below short).
const fn field(name: &'static str, low: f32, high: f32) -> ObsField {
    ObsField { name, low, high }
}

/// A terrain probe field: ground height relative to the centre of mass, capped.
const fn probe_field(name: &'static str) -> ObsField {
    field(name, -TERRAIN_PROBE_CAP_M, TERRAIN_PROBE_CAP_M)
}

/// The observation layout (SPEC §9.2). The index of a field is its place in this list.
pub const OBS_SPEC: [ObsField; OBS_DIM] = [
    // 0–2: unit vector of world-down in the body frame.
    field("gravity_dir_body.x", -1.0, 1.0),
    field("gravity_dir_body.y", -1.0, 1.0),
    field("gravity_dir_body.z", -1.0, 1.0),
    // 3–5: rad/s, body frame (roll, yaw, pitch rates).
    field("ang_vel_body.x", -MAX_SPIN_RAD_S, MAX_SPIN_RAD_S),
    field("ang_vel_body.y", -MAX_SPIN_RAD_S, MAX_SPIN_RAD_S),
    field("ang_vel_body.z", -MAX_SPIN_RAD_S, MAX_SPIN_RAD_S),
    // 6–8: m/s, body frame.
    field("lin_vel_body.x", -MAX_SPEED_M_S, MAX_SPEED_M_S),
    field("lin_vel_body.y", -MAX_SPEED_M_S, MAX_SPEED_M_S),
    field("lin_vel_body.z", -MAX_SPEED_M_S, MAX_SPEED_M_S),
    // 9: m, straight down from the centre of mass, capped at 20.
    field("height", 0.0, MAX_HEIGHT_M),
    // 10–13: 0 or 1, wheel order FL, FR, RL, RR.
    field("wheel_contact.fl", 0.0, 1.0),
    field("wheel_contact.fr", 0.0, 1.0),
    field("wheel_contact.rl", 0.0, 1.0),
    field("wheel_contact.rr", 0.0, 1.0),
    // 14–17: 0..1, wheel order.
    field("suspension_compression.fl", 0.0, 1.0),
    field("suspension_compression.fr", 0.0, 1.0),
    field("suspension_compression.rl", 0.0, 1.0),
    field("suspension_compression.rr", 0.0, 1.0),
    // 18–20: the tank and the mode.
    field("air_fraction", 0.0, 1.0),
    field("thrust_scale", 0.0, MAX_THRUST_SCALE),
    field("unlimited_air", 0.0, 1.0),
    // 21–23: Hop and Boost.
    field("hop_active", 0.0, 1.0),
    field("hop_time_left", 0.0, MAX_HOP_TIME_S),
    field("boost_active", 0.0, 1.0),
    // 24–25: the driver, −1..1 (brake is negative throttle).
    field("driver_throttle", -1.0, 1.0),
    field("driver_steer", -1.0, 1.0),
    // 26–30: m, ground height relative to the centre of mass 2, 4, 6, 8, 10 m ahead.
    probe_field("terrain_probe.2m"),
    probe_field("terrain_probe.4m"),
    probe_field("terrain_probe.6m"),
    probe_field("terrain_probe.8m"),
    probe_field("terrain_probe.10m"),
    // 31–40: the last 10 controller throttles, 0..1, nozzle order.
    field("prev_action.l_fl", 0.0, 1.0),
    field("prev_action.l_fr", 0.0, 1.0),
    field("prev_action.l_rl", 0.0, 1.0),
    field("prev_action.l_rr", 0.0, 1.0),
    field("prev_action.s_fl", 0.0, 1.0),
    field("prev_action.s_rl", 0.0, 1.0),
    field("prev_action.s_fr", 0.0, 1.0),
    field("prev_action.s_rr", 0.0, 1.0),
    field("prev_action.b_l", 0.0, 1.0),
    field("prev_action.b_r", 0.0, 1.0),
];

/// A short fingerprint of the layout: FNV-1a (64-bit) over every field's name, low and
/// high in order, as 16 hex digits. Same layout → same hash; a renamed, moved or
/// re-ranged field gives a new one, so a policy never runs on inputs clamped differently
/// from those it was trained on.
pub fn obs_schema_hash() -> String {
    let mut hash = FNV_OFFSET_BASIS;
    for obs_field in &OBS_SPEC {
        let line = format!("{} {} {}\n", obs_field.name, obs_field.low, obs_field.high);
        hash = fnv1a_add_bytes(hash, line.as_bytes());
    }
    format!("{hash:016x}")
}

/// Fills an observation in `OBS_SPEC` order, every value clamped to its range.
/// `state` is the car now (`World::car_state`); callers usually have it already.
pub fn observe(world: &World, state: &CarState) -> Observation {
    let hop_boost = world.hop_boost();
    let input = world.last_input();
    let mut writer = ObsWriter::new();
    writer.put_all(&state.gravity_dir_body);
    writer.put_all(&state.angular_velocity_body_rad_s);
    writer.put_all(&state.linear_velocity_body_m_s);
    writer.put(state.height_m);
    writer.put_all(&state.wheel_contact.map(one_or_zero));
    writer.put_all(&world.suspension_compressions());
    writer.put(world.air_fraction());
    writer.put(state.mode_thrust_scale);
    writer.put(one_or_zero(world.mode().unlimited_air));
    writer.put(one_or_zero(hop_boost.hop_active()));
    writer.put(hop_boost.hop_time_left_s());
    writer.put(one_or_zero(hop_boost.boost_active()));
    writer.put(input.throttle_fraction());
    writer.put(input.steer_fraction());
    writer.put_all(&world.terrain_probe_heights_m());
    writer.put_all(&world.controller_throttles());
    debug_assert_eq!(writer.next_index, OBS_DIM, "observe filled the wrong count");
    writer.observation
}

/// Writes values into an observation one after another, clamping each to the range
/// of the field at its place.
struct ObsWriter {
    observation: Observation,
    next_index: usize,
}

impl ObsWriter {
    /// An all-zero observation, ready for the first field.
    fn new() -> ObsWriter {
        ObsWriter {
            observation: [0.0; OBS_DIM],
            next_index: 0,
        }
    }

    /// Writes one value into the next field.
    fn put(&mut self, value: f32) {
        if let Some(range) = OBS_SPEC.get(self.next_index) {
            self.observation[self.next_index] = value.clamp(range.low, range.high);
        }
        self.next_index += 1;
    }

    /// Writes several values into the next fields.
    fn put_all(&mut self, values: &[f32]) {
        for value in values {
            self.put(*value);
        }
    }
}

/// 1.0 for true, 0.0 for false.
fn one_or_zero(flag: bool) -> f32 {
    if flag { 1.0 } else { 0.0 }
}
