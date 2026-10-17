//! The simulated world: the ground, the car, and the fixed physics step.
//!
//! `World::step` advances exactly one physics step of `PHYSICS_DT_S` (1/120 s).
//! M1 has only flat ground and driving; thrusters, the air tank and events join
//! the step in M3, following the order in SPEC §7.1.
//!
//! `snapshot` and `restore` save and load the whole world as bytes, for training
//! checkpoints and the determinism tests. The game's respawn is lighter: it only puts
//! the car back at its `Checkpoint` (SPEC §7.9).

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rapier3d::prelude::*;
use serde::{Deserialize, Serialize};

use crate::car::Car;
use crate::chaos::apply_chaos_knock;
use crate::checkpoint::Checkpoint;
use crate::contacts::{MotionBeforeStep, read_body_contacts};
use crate::conventions::{NOZZLE_COUNT, PHYSICS_DT_S, PHYSICS_HZ, WHEEL_COUNT, gravity_vector};
use crate::events::{EventInputs, EventTracker, SimEvent};
use crate::hop_boost::{HopBlockedReason, HopBoost, combine_throttles};
use crate::input::{BUTTON_BOOST, BUTTON_CHAOS, BUTTON_HOP, BUTTON_RESET, InputFrame};
use crate::modes::ModeParams;
use crate::tank::AirTank;
use crate::vehicle_config::VehicleConfig;

/// Ground friction for flat-ground worlds when nothing else is chosen (SPEC §7.2).
/// It only matters when the car's body scrapes the ground.
pub const DEFAULT_GROUND_FRICTION: f32 = 1.0;

/// Half-size of the flat ground box, in metres. Big enough that M1 tests never
/// drive off it. Course pieces replace this in M7.
const FLAT_GROUND_HALF_EXTENTS_M: [f32; 3] = [2000.0, 0.5, 2000.0];

/// Where the car starts on flat ground: centre of mass a little above its resting
/// height (about 0.6 m) so it settles onto its wheels.
const FLAT_GROUND_START_HEIGHT_M: f32 = 0.7;

/// Where something is and which way it faces, in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    /// Centre position.
    pub position_m: [f32; 3],
    /// Orientation as a quaternion (x, y, z, w), the order three.js uses.
    pub rotation_xyzw: [f32; 4],
}

impl Pose {
    /// Builds a pose from Rapier's position vector and rotation quaternion.
    pub fn new(position_m: Vector, rotation: Rotation) -> Pose {
        Pose {
            position_m: position_m.to_array(),
            rotation_xyzw: [rotation.x, rotation.y, rotation.z, rotation.w],
        }
    }
}

/// Most events kept waiting for `drain_events`; older ones are dropped.
const MAX_PENDING_EVENTS: usize = 256;

/// Version of the snapshot byte format. Bump it whenever `World`, `Car` or anything
/// they contain changes shape, so old snapshots are refused instead of misread.
pub const SNAPSHOT_FORMAT_VERSION: u32 = 5;

/// Why a snapshot could not be made or loaded.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("world snapshot could not be encoded or decoded: {0}")]
    Encoding(#[from] postcard::Error),

    #[error("world snapshot has format version {found}, but this build reads version {expected}")]
    WrongVersion { found: u32, expected: u32 },

    #[error("world snapshot has {extra_bytes} unexpected bytes after the end")]
    TrailingBytes { extra_bytes: usize },
}

/// Why the car could not be placed.
#[derive(Debug, thiserror::Error)]
pub enum PlaceCarError {
    #[error("place_car: `{what}` contains a NaN or infinite number")]
    NotFinite { what: &'static str },

    #[error("place_car: the rotation quaternion has zero length; use [0, 0, 0, 1] for no rotation")]
    ZeroRotation,
}

/// A world with flat ground and one car.
#[derive(Serialize, Deserialize)]
pub struct World {
    physics: PhysicsWorld,
    car: Car,
    /// The throttles the controller asked for last; held until it asks again (SPEC §7.1).
    controller_throttles: [f32; NOZZLE_COUNT],
    /// The physics preset in use (Realistic, Arcade or anything in between).
    mode: ModeParams,
    tank: AirTank,
    hop_boost: HopBoost,
    events: EventTracker,
    /// Events not yet collected by `drain_events`. Not saved in snapshots, so a
    /// restored world never reports the same event twice.
    #[serde(skip)]
    pending_events: Vec<SimEvent>,
    /// Last step's input, to tell a new button press from a held button.
    previous_input: InputFrame,
    /// Where the car respawns after a crash or a reset.
    checkpoint: Checkpoint,
    /// Physics steps since the crash, while the wreck waits.
    steps_since_crash: u32,
    /// If set, a crashed car respawns by itself after this many steps (the game turns
    /// it on; training ends the episode at the crash instead). A setting of whoever owns
    /// the world, not state, so it is not in snapshots and `restore` keeps it.
    #[serde(skip)]
    auto_respawn_after_steps: Option<u32>,
    /// The only source of randomness in the sim (CLAUDE.md golden rule 2).
    rng: ChaCha8Rng,
    step_count: u64,
}

impl World {
    /// Builds flat ground and puts the car at the origin, facing world +X, with a full
    /// tank for `mode`. `seed` starts the random generator (used by Chaos), so the same
    /// seed and inputs always give the same run.
    ///
    /// `ground_friction` applies when the car's body scrapes the ground. Tyre grip
    /// comes from `wheels.friction_slip` in the vehicle config instead, because
    /// Rapier's raycast wheels do not read collider friction.
    pub fn new_on_flat_ground(
        vehicle_config: &VehicleConfig,
        mode: ModeParams,
        ground_friction: f32,
        seed: u64,
    ) -> World {
        let mut physics = PhysicsWorld::new();
        physics.gravity = gravity_vector();
        physics.integration_parameters.dt = PHYSICS_DT_S;
        physics.insert_collider(build_flat_ground(ground_friction), None);

        let start_m = Vector::new(0.0, FLAT_GROUND_START_HEIGHT_M, 0.0);
        let car = Car::spawn(&mut physics, vehicle_config, start_m, 0.0);
        // Fill the scene-query tree now; otherwise the wheel rays on the very first
        // step would find nothing, because Rapier only refreshes it inside a step.
        physics.detect_collisions(&(), &());
        let checkpoint = Checkpoint::of_chassis(&physics.bodies[car.chassis]);

        World {
            physics,
            car,
            controller_throttles: [0.0; NOZZLE_COUNT],
            mode,
            tank: AirTank::full(&mode),
            hop_boost: HopBoost::default(),
            events: EventTracker::default(),
            pending_events: Vec::new(),
            previous_input: InputFrame::default(),
            checkpoint,
            steps_since_crash: 0,
            auto_respawn_after_steps: None,
            rng: ChaCha8Rng::seed_from_u64(seed),
            step_count: 0,
        }
    }

    /// Advances the world by one physics step (1/120 s). After a crash the wreck stays
    /// frozen until it respawns (SPEC §7.9): by itself if `enable_auto_respawn` was
    /// called, or when the Reset button is pressed, `respawn` is called or the car is
    /// placed again.
    pub fn step(&mut self, input: InputFrame) {
        let presses = input.newly_pressed_buttons(self.previous_input);
        self.previous_input = input;
        if presses & BUTTON_RESET != 0 {
            self.respawn();
        }
        if self.is_crashed() {
            self.wait_to_respawn();
        } else {
            self.step_car(input, presses);
        }
        self.step_count += 1;
    }

    /// One physics step of a car that has not crashed, in SPEC §7.1 order.
    /// `presses` are the buttons that went down this step.
    fn step_car(&mut self, input: InputFrame, presses: u16) {
        // 1–2. Driver input and the Hop/Boost feed-forward.
        let wheels_down = self.wheels_touching_count();
        let air_kg = self.tank.air_kg();
        let hop_just_pressed = presses & BUTTON_HOP != 0;
        let boost_held = input.is_pressed(BUTTON_BOOST);
        self.hop_boost.update(
            hop_just_pressed,
            boost_held,
            wheels_down,
            air_kg,
            &self.mode,
            PHYSICS_DT_S,
        );
        if presses & BUTTON_CHAOS != 0 {
            self.knock();
        }
        // 3–4. Held controller throttles plus feed-forward, clamped (SPEC §7.7).
        let feed_forward = self.hop_boost.feed_forward_throttles(&self.mode);
        let commanded = combine_throttles(feed_forward, self.controller_throttles);
        // 5–8. Valve lag, tank gate, nozzle forces, wheels.
        let thrust_multiplier = self.mode.thrust_scale * self.tank.thrust_gate(&self.mode);
        let thrust_n =
            self.car
                .prepare_physics_step(&mut self.physics, input, commanded, thrust_multiplier);
        // 9. The physics step. Impacts are judged on the speed from just before it.
        let motion_before = MotionBeforeStep::of(&self.physics.bodies[self.car.chassis]);
        self.physics.step();
        let wheels_down_after = self.wheels_touching_count();
        // 10. The tank: pay for this step's thrust, then refill if on the ground.
        let exhaust_m_s = self.car.exhaust_speed_m_s();
        self.tank
            .use_air(thrust_n, exhaust_m_s, PHYSICS_DT_S, &self.mode);
        self.tank
            .refill(wheels_down_after, PHYSICS_DT_S, &self.mode);
        // 11. Events (SPEC §7.9).
        self.update_events(&motion_before, wheels_down_after);
    }

    /// Gives the car a random Chaos knock (SPEC §7.12).
    fn knock(&mut self) {
        let body = &mut self.physics.bodies[self.car.chassis];
        apply_chaos_knock(body, &self.car.config().chaos, &mut self.rng);
    }

    /// Counts the frozen wreck's time and respawns once the auto-respawn delay is up.
    fn wait_to_respawn(&mut self) {
        self.steps_since_crash = self.steps_since_crash.saturating_add(1);
        if let Some(delay_steps) = self.auto_respawn_after_steps
            && self.steps_since_crash >= delay_steps
        {
            self.respawn();
        }
    }

    /// Makes a crashed car respawn by itself after `delay_s` of sim time (the game's
    /// explode-and-respawn rule, SPEC §7.9). Off by default.
    pub fn enable_auto_respawn(&mut self, delay_s: f32) {
        let delay_steps = (delay_s * PHYSICS_HZ as f32).round() as u32;
        self.auto_respawn_after_steps = Some(delay_steps);
    }

    /// Puts the car back at the last checkpoint with a full tank, clears any crash and
    /// reports `SimEvent::Respawn` (SPEC §7.9). Mode, clock and random generator carry on.
    pub fn respawn(&mut self) {
        self.move_car_to(self.checkpoint);
        self.refill_tank();
        self.keep_event(SimEvent::Respawn);
    }

    /// Makes the car's current pose and motion the place it respawns. Does nothing
    /// while the car is crashed, so a wreck never becomes a respawn point.
    pub fn set_checkpoint(&mut self) {
        if self.is_crashed() {
            return;
        }
        self.checkpoint = Checkpoint::of_chassis(&self.physics.bodies[self.car.chassis]);
    }

    /// Feeds this step's state to the event rules and keeps any new events.
    fn update_events(&mut self, motion_before: &MotionBeforeStep, wheels_down: usize) {
        let contacts = read_body_contacts(
            &self.physics,
            self.car.body_collider,
            self.car.roof_collider,
            motion_before,
        );
        let body = &self.physics.bodies[self.car.chassis];
        let inputs = EventInputs {
            wheels_down,
            tilt_rad: self.tilt_rad(),
            spin_rad_s: body.angvel().length(),
            air_used_kg: self.tank.air_used_kg(),
            height_m: body.translation().y,
            impact_speed_m_s: contacts.impact_speed_m_s,
            roof_touching: contacts.roof_touching,
        };
        let config = &self.car.config().events;
        self.events
            .update(&inputs, config, PHYSICS_DT_S, &mut self.pending_events);
        self.drop_events_over_the_cap();
        if self.events.crashed() {
            // The wreck stays frozen, so close its valves rather than show live flames.
            self.reset_controls();
        }
    }

    /// Queues one event for `drain_events`.
    fn keep_event(&mut self, event: SimEvent) {
        self.pending_events.push(event);
        self.drop_events_over_the_cap();
    }

    /// Callers that never drain (e.g. a headless benchmark) must not grow memory
    /// forever, so only the newest events are kept.
    fn drop_events_over_the_cap(&mut self) {
        if self.pending_events.len() > MAX_PENDING_EVENTS {
            let extra = self.pending_events.len() - MAX_PENDING_EVENTS;
            self.pending_events.drain(..extra);
        }
    }

    /// Returns and forgets the events since the last call (SPEC §7.9).
    pub fn drain_events(&mut self) -> Vec<SimEvent> {
        std::mem::take(&mut self.pending_events)
    }

    /// True from a crash until the car respawns or is placed again.
    pub fn is_crashed(&self) -> bool {
        self.events.crashed()
    }

    /// Angle between the car's up axis and world up: 0 level, π upside down.
    pub fn tilt_rad(&self) -> f32 {
        self.car.up_alignment(&self.physics).clamp(-1.0, 1.0).acos()
    }

    /// Switches to another mode's physics, e.g. Realistic ↔ Arcade. The air left
    /// carries over, limited to the new tank's capacity.
    pub fn set_mode(&mut self, mode: ModeParams) {
        let air_kg = self.tank.air_kg();
        self.mode = mode;
        self.tank.set_air_kg(air_kg, &mode);
    }

    /// The physics preset in use.
    pub fn mode(&self) -> ModeParams {
        self.mode
    }

    /// Fills the tank, e.g. when the car respawns.
    pub fn refill_tank(&mut self) {
        let capacity_kg = self.mode.tank_capacity_kg;
        self.tank.set_air_kg(capacity_kg, &self.mode);
    }

    /// Sets the air left, e.g. for training scenarios. Clamped to 0..capacity.
    pub fn set_air_kg(&mut self, air_kg: f32) {
        self.tank.set_air_kg(air_kg, &self.mode);
    }

    /// Air left in the tank.
    pub fn air_kg(&self) -> f32 {
        self.tank.air_kg()
    }

    /// True once the tank is so low that thrust fades (below `empty_fade_fraction`,
    /// SPEC §7.5); never with unlimited air. The HUD shows "NO AIR" then.
    pub fn is_out_of_air(&self) -> bool {
        self.tank.thrust_gate(&self.mode) < 1.0
    }

    /// Air left as a fraction of capacity; 1 when air is unlimited.
    pub fn air_fraction(&self) -> f32 {
        self.tank.air_fraction(&self.mode)
    }

    /// All the air used since the world was built.
    pub fn air_used_kg(&self) -> f32 {
        self.tank.air_used_kg()
    }

    /// The Hop and Boost state: timers and whether each is active.
    pub fn hop_boost(&self) -> &HopBoost {
        &self.hop_boost
    }

    /// Why a Hop would be refused right now, or `None` if it is allowed.
    pub fn hop_blocked_reason(&self) -> Option<HopBlockedReason> {
        let wheels_down = self.wheels_touching_count();
        self.hop_boost
            .hop_blocked_reason(wheels_down, self.tank.air_kg(), &self.mode)
    }

    /// How many wheels touch the ground (0..4).
    pub fn wheels_touching_count(&self) -> usize {
        let touching = self.wheels_touching();
        touching.iter().filter(|wheel_down| **wheel_down).count()
    }

    /// Sets the throttles (0..1 per nozzle, in nozzle order) that the controller wants.
    /// They are held until the next call. Controllers run at 60 Hz, so callers set
    /// them every `PHYSICS_STEPS_PER_CONTROL_STEP` physics steps (SPEC §7.1).
    pub fn set_controller_throttles(&mut self, throttles: [f32; NOZZLE_COUNT]) {
        self.controller_throttles = throttles;
    }

    /// How open each thruster valve is now, 0..1, in nozzle order. The renderer sizes
    /// the flames from these.
    pub fn effective_throttles(&self) -> [f32; NOZZLE_COUNT] {
        self.car.effective_throttles()
    }

    /// Moves the car to a new position, orientation and velocity, for tests and for
    /// training scenarios (SPEC §9.6). Velocities are in world coordinates.
    ///
    /// Fails on NaN or infinite values or a zero rotation. Otherwise it forgets any
    /// undrained events and resets the car as `move_car_to` describes; the tank is
    /// left as it is (see `refill_tank`), and so is the checkpoint (see `set_checkpoint`).
    pub fn place_car(
        &mut self,
        position_m: [f32; 3],
        rotation_xyzw: [f32; 4],
        linear_velocity_m_s: [f32; 3],
        angular_velocity_rad_s: [f32; 3],
    ) -> Result<(), PlaceCarError> {
        check_finite("position_m", &position_m)?;
        check_finite("rotation_xyzw", &rotation_xyzw)?;
        check_finite("linear_velocity_m_s", &linear_velocity_m_s)?;
        check_finite("angular_velocity_rad_s", &angular_velocity_rad_s)?;
        let [x, y, z, w] = rotation_xyzw;
        if Rotation::from_xyzw(x, y, z, w).length() < 1e-6 {
            return Err(PlaceCarError::ZeroRotation);
        }

        self.pending_events.clear();
        self.move_car_to(Checkpoint {
            pose: Pose {
                position_m,
                rotation_xyzw,
            },
            linear_velocity_m_s,
            angular_velocity_rad_s,
        });
        Ok(())
    }

    /// Clears the held controller throttles, closes every valve, ends any Hop or Boost
    /// and its cooldown, and centres the steering.
    fn reset_controls(&mut self) {
        self.controller_throttles = [0.0; NOZZLE_COUNT];
        self.hop_boost = HopBoost::default();
        self.car.reset_actuators();
    }

    /// Moves the car and resets the held controller throttles, valves, Hop/Boost,
    /// steering and event rules (including a crash), so nothing carries over. The tank
    /// is left as it is. Shared by `place_car` and `respawn`; the values must be finite
    /// with a non-zero rotation.
    fn move_car_to(&mut self, target: Checkpoint) {
        self.reset_controls();
        self.events.reset();
        self.steps_since_crash = 0;
        let [x, y, z, w] = target.pose.rotation_xyzw;
        let rotation = Rotation::from_xyzw(x, y, z, w).normalize();
        let body = &mut self.physics.bodies[self.car.chassis];
        body.set_position(
            Pose3::from_parts(Vector::from_array(target.pose.position_m), rotation),
            true,
        );
        body.set_linvel(Vector::from_array(target.linear_velocity_m_s), true);
        body.set_angvel(Vector::from_array(target.angular_velocity_rad_s), true);
        // Refresh the scene-query tree so the wheel rays see the car's new place.
        self.physics.detect_collisions(&(), &());
    }

    /// Saves the complete state of the world as bytes: the format version, then the world.
    pub fn snapshot(&self) -> Result<Vec<u8>, SnapshotError> {
        Ok(postcard::to_allocvec(&(SNAPSHOT_FORMAT_VERSION, self))?)
    }

    /// Replaces this world with one saved by `snapshot`. Stepping the restored world
    /// with the same inputs gives bit-identical results to the original.
    /// On any error this world is left unchanged.
    pub fn restore(&mut self, snapshot: &[u8]) -> Result<(), SnapshotError> {
        let (version, after_version): (u32, &[u8]) = postcard::take_from_bytes(snapshot)?;
        if version != SNAPSHOT_FORMAT_VERSION {
            return Err(SnapshotError::WrongVersion {
                found: version,
                expected: SNAPSHOT_FORMAT_VERSION,
            });
        }
        let (world, leftover): (World, &[u8]) = postcard::take_from_bytes(after_version)?;
        if !leftover.is_empty() {
            return Err(SnapshotError::TrailingBytes {
                extra_bytes: leftover.len(),
            });
        }
        let auto_respawn_after_steps = self.auto_respawn_after_steps;
        *self = world;
        self.auto_respawn_after_steps = auto_respawn_after_steps;
        Ok(())
    }

    /// Number of physics steps taken since the world was built.
    pub fn step_count(&self) -> u64 {
        self.step_count
    }

    /// Simulated time since the world was built.
    pub fn time_s(&self) -> f64 {
        // Divide by the exact rate; PHYSICS_DT_S is a rounded f32.
        self.step_count as f64 / PHYSICS_HZ as f64
    }

    /// The car's current position and orientation (its centre of mass).
    pub fn chassis_pose(&self) -> Pose {
        let body = &self.physics.bodies[self.car.chassis];
        Pose::new(body.translation(), *body.rotation())
    }

    /// Where each wheel is drawn, in wheel order FL, FR, RL, RR. Includes steering
    /// and how far each wheel has rolled, so the renderer can spin them.
    pub fn wheel_poses(&self) -> [Pose; WHEEL_COUNT] {
        self.car.wheel_poses(&self.physics)
    }

    /// The car's velocity in world coordinates.
    pub fn linear_velocity_m_s(&self) -> [f32; 3] {
        self.physics.bodies[self.car.chassis].linvel().to_array()
    }

    /// The car's angular velocity in world coordinates.
    pub fn angular_velocity_rad_s(&self) -> [f32; 3] {
        self.physics.bodies[self.car.chassis].angvel().to_array()
    }

    /// The car's angular velocity in its own body frame: [roll rate about +X,
    /// yaw rate about +Y, pitch rate about +Z] (SPEC §4 signs).
    pub fn angular_velocity_body_rad_s(&self) -> [f32; 3] {
        let body = &self.physics.bodies[self.car.chassis];
        let world_rate = body.angvel();
        (body.rotation().inverse() * world_rate).to_array()
    }

    /// Speed along the car's nose direction. Negative when rolling backwards.
    pub fn forward_speed_m_s(&self) -> f32 {
        self.car.forward_speed_m_s(&self.physics)
    }

    /// Which wheels touch the ground, in wheel order FL, FR, RL, RR.
    pub fn wheels_touching(&self) -> [bool; WHEEL_COUNT] {
        self.car.wheels_touching(&self.physics)
    }

    /// Current front-wheel angle. Positive turns left.
    pub fn steering_angle_rad(&self) -> f32 {
        self.car.steering_angle_rad()
    }
}

/// Fails if any value is NaN or infinite.
fn check_finite(what: &'static str, values: &[f32]) -> Result<(), PlaceCarError> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(PlaceCarError::NotFinite { what })
    }
}

/// A large flat box whose top surface is at y = 0.
fn build_flat_ground(friction: f32) -> ColliderBuilder {
    let [half_x, half_y, half_z] = FLAT_GROUND_HALF_EXTENTS_M;
    ColliderBuilder::cuboid(half_x, half_y, half_z)
        .translation(Vector::new(0.0, -half_y, 0.0))
        .friction(friction)
}
