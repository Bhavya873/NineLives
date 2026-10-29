//! Game events: airborne, upset, save, hard landing and crash (SPEC §7.9).
//!
//! `EventTracker` watches a few numbers each physics step (wheels down, tilt, spin,
//! impacts, suspension) and emits `SimEvent`s when a rule is met. It does no physics
//! itself, so the rules can be tested on plain numbers. After a crash it stays quiet
//! until reset.

use serde::{Deserialize, Serialize};

use crate::car_state::Touchdown;
use crate::conventions::WHEEL_COUNT;
use crate::landing::{Landing, LandingQuality, TakeOff};
use crate::vehicle_config::EventConfig;

/// Why the car crashed (SPEC §7.9). Results files spell it in snake_case ("hard_impact").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrashCause {
    /// The body or roof hit something too fast.
    HardImpact,
    /// The roof touched something while the car was upside down.
    UpsideDown,
    /// The car stayed tipped over for too long.
    Stuck,
    /// The car fell out of the world or left the course's bounds box.
    OutOfBounds,
}

/// Something that happened, for the HUD, the stats and the reward.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SimEvent {
    /// The last wheel left the ground.
    AirborneStart,
    /// A wheel touched the ground again.
    AirborneEnd,
    /// The car was knocked off balance (in the air too long, or tilted too far).
    UpsetStart,
    /// The car recovered from an upset onto its wheels (SPEC §7.9).
    Save {
        /// From the upset starting to the save being confirmed.
        duration_s: f32,
        /// Air used during the upset.
        air_used_kg: f32,
        /// The largest tilt during the upset.
        peak_tilt_rad: f32,
        /// How the car was moving just before the touchdown that led to the save: the
        /// first wheel touchdown after no wheel touched, or, if the wheels never left the
        /// ground, the moment all four were down again (SPEC §9.4 save quality).
        landing: Touchdown,
        /// How well that landing kept the take-off heading and speed (PLAN §3.3).
        quality: LandingQuality,
        /// The raw (per-tick, unsmoothed) peak seat g of the latest stunt window, in g
        /// (`docs/contracts/g-meter.md`); 0 if no airtime has started since the reset.
        peak_g: f32,
    },
    /// A wheel's suspension bottomed out just after a touchdown (PLAN §3.3). Not a
    /// crash: the car drives on, but the reward charges for it.
    HardLanding {
        /// The largest suspension compression of the four wheels, 0..1.
        compression: f32,
        /// Fall speed just before the touchdown (positive downward), in m/s.
        fall_m_s: f32,
    },
    /// The car crashed. In the game it explodes and respawns; in training the episode ends.
    Crash { cause: CrashCause },
    /// The car was put back at the last checkpoint, after a crash or the Reset button.
    Respawn,
    /// The car crossed a course checkpoint line (0 is the first, in file order); the
    /// respawn point is now there (SPEC §7.11).
    CheckpointReached { number: u32 },
    /// The car crossed the finish line, `time_s` after the world started.
    Finished { time_s: f32 },
    /// A kick plate (`piece` index in the course file) pushed the car.
    Kicked { piece: u32 },
}

/// What the tracker looks at on each physics step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EventInputs {
    /// How many wheels touch the ground (0..4).
    pub wheels_down: usize,
    /// Angle between the car's up axis and world up.
    pub tilt_rad: f32,
    /// Size of the angular velocity.
    pub spin_rad_s: f32,
    /// The motion from just before this step: spin size and fall speed (positive
    /// downward). A touchdown is judged on these, since the step itself stops the car.
    pub motion_before_step: Touchdown,
    /// All the air used so far.
    pub air_used_kg: f32,
    /// Height of the centre of mass.
    pub height_m: f32,
    /// Fastest approach speed of the body or roof into anything this step.
    pub impact_speed_m_s: f32,
    /// True if the roof touches anything.
    pub roof_touching: bool,
    /// The car's nose direction (body +X) in the world frame, unit length.
    pub nose_world: [f32; 3],
    /// Speed along the nose, in m/s; negative when rolling backwards.
    pub forward_speed_m_s: f32,
    /// The largest suspension compression of the four wheels, 0 (extended or not
    /// touching) to 1 (bottomed out).
    pub max_suspension_compression: f32,
    /// True if the centre of mass has left the course's bounds box (always false on
    /// flat ground, which has no box).
    pub outside_course: bool,
    /// The seat g-load of this step, unsmoothed, in g (SPEC §9.4).
    pub raw_g: f32,
    /// The same g-load smoothed over `[passenger] smoothing_s`: what the meter shows.
    pub displayed_g: f32,
}

/// An upset in progress.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Upset {
    elapsed_s: f32,
    air_used_at_start_kg: f32,
    peak_tilt_rad: f32,
    /// How long the save conditions have held so far.
    save_held_s: f32,
    /// The touchdown the save will be judged on, once there is one.
    landing: Option<Landing>,
}

/// Remembers what is needed between steps to apply the SPEC §7.9 rules.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EventTracker {
    /// Time since the last wheel left the ground; 0 while any wheel touches.
    airborne_s: f32,
    upset: Option<Upset>,
    stuck_s: f32,
    crashed: bool,
    /// Wheels down on the step before, to spot a touchdown.
    previous_wheels_down: usize,
    /// The latest take-off (AirborneStart), which landings are judged against. Cleared
    /// once all four wheels are down again, so a later upset (say, tipped by a bump) is
    /// not judged against an old jump.
    take_off: Option<TakeOff>,
    /// False until the first step after a reset, when there is no "step before" to
    /// compare wheel contact with: a car placed on its wheels has not touched down.
    stepped_since_reset: bool,
    /// Time left to look for a bottomed-out wheel after the latest touchdown, in s;
    /// 0 when not looking.
    hard_landing_watch_s: f32,
    /// Fall speed just before the latest touchdown, for the HardLanding event, in m/s.
    touchdown_fall_m_s: f32,
    /// The stunt window for the g-meter (`docs/contracts/g-meter.md`).
    stunt_window: StuntWindow,
    /// Largest raw seat g in the stunt window so far, in g. Kept after the window
    /// closes, until the next AirborneStart or a reset.
    peak_raw_g: f32,
    /// Largest smoothed (displayed) seat g in the same window, in g.
    peak_displayed_g: f32,
}

/// How long the stunt window stays open after the touchdown that ends the airtime, in s.
const STUNT_WINDOW_TAIL_S: f32 = 0.5;

/// Where the g-meter's stunt window stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
enum StuntWindow {
    /// No airtime since the reset, or the window has closed.
    #[default]
    Closed,
    /// Open: from AirborneStart through the airtime.
    Open,
    /// The airtime ended; the window closes after this many more seconds.
    Closing { left_s: f32 },
}

impl EventTracker {
    /// The raw (per-tick) peak seat g of the stunt window, in g; 0 before the first
    /// take-off after a reset.
    pub fn peak_raw_g(&self) -> f32 {
        self.peak_raw_g
    }

    /// The peak of the displayed (smoothed) seat g over the same window, in g.
    pub fn peak_displayed_g(&self) -> f32 {
        self.peak_displayed_g
    }

    /// True once a crash has happened. Nothing else is reported until `reset`.
    pub fn crashed(&self) -> bool {
        self.crashed
    }

    /// The latest take-off, until all four wheels are down again (see `take_off`).
    pub fn take_off(&self) -> Option<TakeOff> {
        self.take_off
    }

    /// Forgets everything, e.g. when the car is placed somewhere new.
    pub fn reset(&mut self) {
        *self = EventTracker::default();
    }

    /// Applies the rules for one step of `dt_s` and adds any new events to `events`.
    pub fn update(
        &mut self,
        inputs: &EventInputs,
        config: &EventConfig,
        dt_s: f32,
        events: &mut Vec<SimEvent>,
    ) {
        if self.crashed {
            return;
        }
        self.update_stuck_timer(inputs, config, dt_s);
        if let Some(cause) = self.crash_cause(inputs, config) {
            self.crashed = true;
            self.upset = None;
            self.update_stunt_window(inputs, dt_s);
            events.push(SimEvent::Crash { cause });
            return;
        }
        self.update_airborne(inputs, dt_s, events);
        self.update_stunt_window(inputs, dt_s);
        self.update_hard_landing(inputs, config, dt_s, events);
        self.update_upset_and_save(inputs, config, dt_s, events);
        if inputs.wheels_down == WHEEL_COUNT {
            // Any landing has been judged by now (the upset's landing is set on the
            // first touchdown), so the take-off has done its job.
            self.take_off = None;
        }
        self.previous_wheels_down = inputs.wheels_down;
        self.stepped_since_reset = true;
    }

    /// The first crash rule that applies this step, if any (SPEC §7.9).
    fn crash_cause(&self, inputs: &EventInputs, config: &EventConfig) -> Option<CrashCause> {
        if inputs.impact_speed_m_s > config.crash_impact_speed_m_s {
            return Some(CrashCause::HardImpact);
        }
        if inputs.roof_touching && inputs.tilt_rad > config.upside_down_tilt_deg.to_radians() {
            return Some(CrashCause::UpsideDown);
        }
        if inputs.height_m < config.out_of_bounds_y_m || inputs.outside_course {
            return Some(CrashCause::OutOfBounds);
        }
        if self.stuck_s > config.stuck_s {
            return Some(CrashCause::Stuck);
        }
        None
    }

    /// Counts how long the car has been tipped past the stuck tilt (SPEC §7.9).
    fn update_stuck_timer(&mut self, inputs: &EventInputs, config: &EventConfig, dt_s: f32) {
        if inputs.tilt_rad > config.stuck_tilt_deg.to_radians() {
            self.stuck_s += dt_s;
        } else {
            self.stuck_s = 0.0;
        }
    }

    fn is_airborne(&self) -> bool {
        self.airborne_s > 0.0
    }

    /// True on the step the first wheel touches after none did (never on the first
    /// step after a reset, which has no step before).
    fn touched_down(&self, inputs: &EventInputs) -> bool {
        self.stepped_since_reset && self.previous_wheels_down == 0 && inputs.wheels_down > 0
    }

    /// Airborne starts when no wheel touches, and ends when any wheel touches again.
    /// Airborne start also records the take-off; its forward speed counts only if the
    /// wheels were on the ground the step before (not for a start in the air).
    fn update_airborne(&mut self, inputs: &EventInputs, dt_s: f32, events: &mut Vec<SimEvent>) {
        let airborne_now = inputs.wheels_down == 0;
        if airborne_now && !self.is_airborne() {
            events.push(SimEvent::AirborneStart);
            // A new take-off starts a fresh stunt window, even inside the old one's tail.
            self.stunt_window = StuntWindow::Open;
            self.peak_raw_g = 0.0;
            self.peak_displayed_g = 0.0;
            let left_the_wheels = self.previous_wheels_down > 0;
            let speed_m_s = left_the_wheels.then_some(inputs.forward_speed_m_s);
            self.take_off = Some(TakeOff::new(inputs.nose_world, speed_m_s));
        }
        if !airborne_now && self.is_airborne() {
            events.push(SimEvent::AirborneEnd);
            self.stunt_window = StuntWindow::Closing {
                left_s: STUNT_WINDOW_TAIL_S,
            };
        }
        if airborne_now {
            self.airborne_s += dt_s;
        } else {
            self.airborne_s = 0.0;
        }
    }

    /// Folds this step's g into the stunt window's peaks while the window is open. The
    /// touchdown step is inside the window, and so are the 0.5 s after it.
    fn update_stunt_window(&mut self, inputs: &EventInputs, dt_s: f32) {
        self.stunt_window = match self.stunt_window {
            StuntWindow::Closed => return,
            StuntWindow::Open => StuntWindow::Open,
            StuntWindow::Closing { left_s } if left_s - dt_s > 0.0 => StuntWindow::Closing {
                left_s: left_s - dt_s,
            },
            StuntWindow::Closing { .. } => StuntWindow::Closed,
        };
        self.peak_raw_g = self.peak_raw_g.max(inputs.raw_g);
        self.peak_displayed_g = self.peak_displayed_g.max(inputs.displayed_g);
    }

    /// PLAN §3.3: after a touchdown, watches for `hard_landing_window_s` for a wheel's
    /// suspension reaching `hard_landing_compression`, and reports it once.
    fn update_hard_landing(
        &mut self,
        inputs: &EventInputs,
        config: &EventConfig,
        dt_s: f32,
        events: &mut Vec<SimEvent>,
    ) {
        if self.touched_down(inputs) {
            self.hard_landing_watch_s = config.hard_landing_window_s;
            self.touchdown_fall_m_s = inputs.motion_before_step.fall_m_s;
        }
        if self.hard_landing_watch_s <= 0.0 {
            return;
        }
        if inputs.max_suspension_compression >= config.hard_landing_compression {
            events.push(SimEvent::HardLanding {
                compression: inputs.max_suspension_compression,
                fall_m_s: self.touchdown_fall_m_s,
            });
            self.hard_landing_watch_s = 0.0;
            return;
        }
        self.hard_landing_watch_s -= dt_s;
    }

    /// Starts an upset, tracks it, and ends it with a save once the car has been
    /// back on its wheels, level and still, for long enough (SPEC §7.9).
    fn update_upset_and_save(
        &mut self,
        inputs: &EventInputs,
        config: &EventConfig,
        dt_s: f32,
        events: &mut Vec<SimEvent>,
    ) {
        let knocked = self.airborne_s > config.upset_airborne_s
            || inputs.tilt_rad > config.upset_tilt_deg.to_radians();
        if self.upset.is_none() && knocked {
            events.push(SimEvent::UpsetStart);
            self.upset = Some(Upset {
                elapsed_s: 0.0,
                air_used_at_start_kg: inputs.air_used_kg,
                peak_tilt_rad: inputs.tilt_rad,
                save_held_s: 0.0,
                landing: None,
            });
        }
        let touched_down = self.touched_down(inputs);
        let take_off = self.take_off;
        let Some(upset) = self.upset.as_mut() else {
            return;
        };
        upset.elapsed_s += dt_s;
        upset.peak_tilt_rad = upset.peak_tilt_rad.max(inputs.tilt_rad);

        let back_on_wheels = inputs.wheels_down == WHEEL_COUNT
            && inputs.tilt_rad < config.save_tilt_deg.to_radians()
            && inputs.spin_rad_s < config.save_max_spin_rad_s;
        let all_wheels_back = inputs.wheels_down == WHEEL_COUNT;
        if touched_down || all_wheels_back {
            upset.landing.get_or_insert_with(|| {
                let motion = inputs.motion_before_step;
                Landing::at_touchdown(motion, take_off, inputs.nose_world, config)
            });
        }
        // Still in the air (or never back on a wheel): nothing to judge or save yet.
        // A save needs all four wheels down, which sets the landing above.
        let Some(landing) = upset.landing.as_mut() else {
            return;
        };
        landing.update(inputs.forward_speed_m_s, config, dt_s);
        upset.save_held_s = if back_on_wheels {
            upset.save_held_s + dt_s
        } else {
            0.0
        };
        if upset.save_held_s >= config.save_hold_s {
            events.push(SimEvent::Save {
                duration_s: upset.elapsed_s,
                air_used_kg: inputs.air_used_kg - upset.air_used_at_start_kg,
                peak_tilt_rad: upset.peak_tilt_rad,
                landing: landing.motion,
                quality: landing.finish(inputs.forward_speed_m_s),
                peak_g: self.peak_raw_g,
            });
            self.upset = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};

    const DT_S: f32 = 1.0 / 120.0;

    fn config() -> EventConfig {
        VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML)
            .unwrap()
            .events
    }

    fn upright_on_wheels() -> EventInputs {
        EventInputs {
            wheels_down: 4,
            tilt_rad: 0.0,
            spin_rad_s: 0.0,
            motion_before_step: Touchdown {
                spin_rad_s: 0.0,
                fall_m_s: 0.0,
            },
            air_used_kg: 0.0,
            height_m: 0.6,
            impact_speed_m_s: 0.0,
            roof_touching: false,
            outside_course: false,
            nose_world: [1.0, 0.0, 0.0],
            forward_speed_m_s: 0.0,
            max_suspension_compression: 0.3,
            raw_g: 1.0,
            displayed_g: 1.0,
        }
    }

    /// Feeds the same inputs for `seconds` and returns all events.
    fn feed(tracker: &mut EventTracker, inputs: EventInputs, seconds: f32) -> Vec<SimEvent> {
        let mut events = Vec::new();
        for _ in 0..(seconds / DT_S).round() as usize {
            tracker.update(&inputs, &config(), DT_S, &mut events);
        }
        events
    }

    /// The motion just before a step: `spin_rad_s` and `fall_m_s`.
    fn moving(spin_rad_s: f32, fall_m_s: f32) -> Touchdown {
        Touchdown {
            spin_rad_s,
            fall_m_s,
        }
    }

    #[test]
    fn a_save_is_judged_on_the_first_touchdown_not_a_gentle_bounce_after_it() {
        let mut tracker = EventTracker::default();
        let in_the_air = EventInputs {
            wheels_down: 0,
            ..upright_on_wheels()
        };
        let hard_touch = EventInputs {
            wheels_down: 2,
            motion_before_step: moving(4.0, 6.0),
            ..upright_on_wheels()
        };
        let gentle_touch = EventInputs {
            motion_before_step: moving(0.2, 0.5),
            ..upright_on_wheels()
        };

        let mut events = feed(&mut tracker, in_the_air, 0.5);
        events.extend(feed(&mut tracker, hard_touch, DT_S));
        events.extend(feed(&mut tracker, in_the_air, 0.2));
        events.extend(feed(&mut tracker, gentle_touch, 1.0));

        let landing = events.iter().find_map(|event| match event {
            SimEvent::Save { landing, .. } => Some(*landing),
            _ => None,
        });
        assert_eq!(landing, Some(moving(4.0, 6.0)));
    }

    #[test]
    fn a_short_jump_then_a_level_landing_is_an_upset_and_a_save() {
        let mut tracker = EventTracker::default();
        let in_the_air = EventInputs {
            wheels_down: 0,
            air_used_kg: 0.0,
            ..upright_on_wheels()
        };

        let mut events = feed(&mut tracker, in_the_air, 1.0);
        let landed = EventInputs {
            air_used_kg: 3.0,
            ..upright_on_wheels()
        };
        events.extend(feed(&mut tracker, landed, 1.0));

        assert_eq!(events[0], SimEvent::AirborneStart);
        assert_eq!(events[1], SimEvent::UpsetStart);
        assert_eq!(events[2], SimEvent::AirborneEnd);
        assert!(matches!(events[3], SimEvent::Save { air_used_kg, .. } if air_used_kg == 3.0));
        assert_eq!(events.len(), 4);
    }

    fn hard_landings(events: &[SimEvent]) -> usize {
        let mut count = 0;
        for event in events {
            if matches!(event, SimEvent::HardLanding { .. }) {
                count += 1;
            }
        }
        count
    }

    #[test]
    fn a_touchdown_that_bottoms_out_a_wheel_is_one_hard_landing_not_a_crash() {
        let mut tracker = EventTracker::default();
        let in_the_air = EventInputs {
            wheels_down: 0,
            ..upright_on_wheels()
        };
        let first_touch = EventInputs {
            wheels_down: 2,
            max_suspension_compression: 0.4,
            ..upright_on_wheels()
        };
        let bottomed_out = EventInputs {
            max_suspension_compression: 1.0,
            ..upright_on_wheels()
        };

        let mut events = feed(&mut tracker, in_the_air, 0.5);
        events.extend(feed(&mut tracker, first_touch, DT_S));
        events.extend(feed(&mut tracker, bottomed_out, 0.1));

        assert_eq!(hard_landings(&events), 1);
        assert!(!tracker.crashed());
    }

    #[test]
    fn a_soft_touchdown_is_not_a_hard_landing() {
        let mut tracker = EventTracker::default();
        let in_the_air = EventInputs {
            wheels_down: 0,
            ..upright_on_wheels()
        };

        let mut events = feed(&mut tracker, in_the_air, 0.5);
        events.extend(feed(&mut tracker, upright_on_wheels(), 1.0));

        assert_eq!(hard_landings(&events), 0);
    }

    #[test]
    fn bottoming_out_on_a_bump_without_leaving_the_ground_is_not_a_hard_landing() {
        let mut tracker = EventTracker::default();
        let bump = EventInputs {
            max_suspension_compression: 1.0,
            ..upright_on_wheels()
        };

        let mut events = feed(&mut tracker, upright_on_wheels(), 1.0);
        events.extend(feed(&mut tracker, bump, 0.1));

        assert_eq!(hard_landings(&events), 0);
    }

    #[test]
    fn a_save_after_landing_turned_90_degrees_reports_the_heading_error() {
        let mut tracker = EventTracker::default();
        let in_the_air = EventInputs {
            wheels_down: 0,
            ..upright_on_wheels()
        };
        let landed_turned = EventInputs {
            nose_world: [0.0, 0.0, 1.0],
            ..upright_on_wheels()
        };

        // Take off from the wheels: only then is there a heading to keep.
        let mut events = feed(&mut tracker, upright_on_wheels(), 0.2);
        events.extend(feed(&mut tracker, in_the_air, 0.5));
        events.extend(feed(&mut tracker, landed_turned, 1.0));

        let quality = events.iter().find_map(|event| match event {
            SimEvent::Save { quality, .. } => Some(*quality),
            _ => None,
        });
        let error_deg = quality.unwrap().heading_error_rad.unwrap().to_degrees();
        assert!((error_deg - 90.0).abs() < 1e-3, "{error_deg}");
    }

    #[test]
    fn a_save_after_a_fast_take_off_reports_the_speed_kept() {
        let mut tracker = EventTracker::default();
        let driving = EventInputs {
            forward_speed_m_s: 20.0,
            ..upright_on_wheels()
        };
        let in_the_air = EventInputs {
            wheels_down: 0,
            ..driving
        };
        let landed_slower = EventInputs {
            forward_speed_m_s: 10.0,
            ..upright_on_wheels()
        };

        let mut events = feed(&mut tracker, driving, 0.5);
        events.extend(feed(&mut tracker, in_the_air, 0.5));
        events.extend(feed(&mut tracker, landed_slower, 1.0));

        let quality = events.iter().find_map(|event| match event {
            SimEvent::Save { quality, .. } => Some(*quality),
            _ => None,
        });
        assert_eq!(quality.unwrap().speed_kept, Some(0.5));
    }

    #[test]
    fn a_tip_over_long_after_a_jump_is_not_judged_against_the_old_take_off() {
        let mut tracker = EventTracker::default();
        let driving = EventInputs {
            forward_speed_m_s: 20.0,
            ..upright_on_wheels()
        };
        let in_the_air = EventInputs {
            wheels_down: 0,
            ..driving
        };
        // After the jump the car drives on, turns a quarter turn, then a bump tips it.
        let turned = EventInputs {
            nose_world: [0.0, 0.0, 1.0],
            ..driving
        };
        let tipped = EventInputs {
            wheels_down: 2,
            tilt_rad: 70_f32.to_radians(),
            ..turned
        };

        feed(&mut tracker, driving, 0.5);
        feed(&mut tracker, in_the_air, 0.5);
        feed(&mut tracker, driving, 2.0);
        feed(&mut tracker, turned, 1.0);
        feed(&mut tracker, tipped, 0.3);
        let events = feed(&mut tracker, turned, 1.0);

        let quality = events.iter().find_map(|event| match event {
            SimEvent::Save { quality, .. } => Some(*quality),
            _ => None,
        });
        assert_eq!(quality.unwrap().heading_error_rad, None);
        assert_eq!(quality.unwrap().speed_kept, None);
    }

    #[test]
    fn the_first_step_after_a_reset_on_the_wheels_is_not_a_touchdown() {
        let mut tracker = EventTracker::default();
        let springs_bottomed = EventInputs {
            max_suspension_compression: 1.0,
            ..upright_on_wheels()
        };
        feed(&mut tracker, upright_on_wheels(), 0.5);
        tracker.reset();

        let events = feed(&mut tracker, springs_bottomed, 0.1);

        assert_eq!(hard_landings(&events), 0);
    }

    #[test]
    fn staying_tipped_over_for_more_than_2_s_is_a_stuck_crash() {
        let mut tracker = EventTracker::default();
        let tipped = EventInputs {
            tilt_rad: 110_f32.to_radians(),
            wheels_down: 0,
            ..upright_on_wheels()
        };

        let events = feed(&mut tracker, tipped, 2.5);

        let crash = SimEvent::Crash {
            cause: CrashCause::Stuck,
        };
        assert!(events.contains(&crash));
        assert!(tracker.crashed());
    }

    #[test]
    fn nothing_is_reported_after_a_crash_until_reset() {
        let mut tracker = EventTracker::default();
        let hard_hit = EventInputs {
            impact_speed_m_s: 9.0,
            ..upright_on_wheels()
        };
        feed(&mut tracker, hard_hit, 0.1);

        let in_the_air = EventInputs {
            wheels_down: 0,
            ..upright_on_wheels()
        };
        let later = feed(&mut tracker, in_the_air, 1.0);
        tracker.reset();
        let after_reset = feed(&mut tracker, in_the_air, 0.1);

        assert!(later.is_empty());
        assert_eq!(after_reset, vec![SimEvent::AirborneStart]);
    }

    /// Inputs with this raw and displayed g.
    fn with_g(inputs: EventInputs, raw_g: f32, displayed_g: f32) -> EventInputs {
        EventInputs {
            raw_g,
            displayed_g,
            ..inputs
        }
    }

    fn in_the_air() -> EventInputs {
        EventInputs {
            wheels_down: 0,
            ..upright_on_wheels()
        }
    }

    #[test]
    fn the_peak_is_zero_until_a_take_off_and_the_touchdown_counts() {
        let mut tracker = EventTracker::default();

        feed(&mut tracker, with_g(upright_on_wheels(), 5.0, 5.0), 0.5);
        assert_eq!(tracker.peak_raw_g(), 0.0);

        feed(&mut tracker, with_g(in_the_air(), 0.0, 0.0), 1.0);
        // One tick of landing spike; the smoothed value barely moves.
        feed(&mut tracker, with_g(upright_on_wheels(), 6.5, 1.4), DT_S);
        feed(&mut tracker, with_g(upright_on_wheels(), 1.0, 1.3), 0.3);

        assert_eq!(tracker.peak_raw_g(), 6.5);
        assert_eq!(tracker.peak_displayed_g(), 1.4);
    }

    #[test]
    fn the_window_closes_half_a_second_after_the_touchdown() {
        let mut tracker = EventTracker::default();
        feed(&mut tracker, in_the_air(), 0.5);
        feed(&mut tracker, with_g(upright_on_wheels(), 2.0, 2.0), DT_S);

        // Still inside the window 0.4 s after the touchdown.
        feed(&mut tracker, with_g(upright_on_wheels(), 3.0, 3.0), 0.4);
        assert_eq!(tracker.peak_raw_g(), 3.0);
        // Past the 0.5 s tail (the touchdown tick was the first of it) nothing counts.
        feed(&mut tracker, with_g(upright_on_wheels(), 1.0, 1.0), 0.15);
        feed(&mut tracker, with_g(upright_on_wheels(), 9.0, 9.0), 0.2);
        assert_eq!(tracker.peak_raw_g(), 3.0);
        // The value is kept after the window closes.
        feed(&mut tracker, with_g(upright_on_wheels(), 9.0, 9.0), 1.0);
        assert_eq!(tracker.peak_raw_g(), 3.0);
    }

    #[test]
    fn the_peak_resets_at_the_next_take_off_and_on_reset() {
        let mut tracker = EventTracker::default();
        feed(&mut tracker, with_g(in_the_air(), 1.0, 1.0), 0.5);
        feed(&mut tracker, with_g(upright_on_wheels(), 7.0, 2.0), DT_S);
        feed(&mut tracker, upright_on_wheels(), 1.0);
        assert_eq!(tracker.peak_raw_g(), 7.0);

        feed(&mut tracker, with_g(in_the_air(), 0.2, 0.2), DT_S);
        assert_eq!(tracker.peak_raw_g(), 0.2);

        tracker.reset();
        assert_eq!(tracker.peak_raw_g(), 0.0);
        assert_eq!(tracker.peak_displayed_g(), 0.0);
    }

    #[test]
    fn a_save_carries_the_raw_peak_of_its_stunt_window() {
        let mut tracker = EventTracker::default();
        let mut events = feed(&mut tracker, with_g(in_the_air(), 0.0, 0.0), 1.0);
        events.extend(feed(
            &mut tracker,
            with_g(upright_on_wheels(), 4.5, 1.2),
            DT_S,
        ));
        events.extend(feed(&mut tracker, upright_on_wheels(), 1.0));

        let peak = events.iter().find_map(|event| match event {
            SimEvent::Save { peak_g, .. } => Some(*peak_g),
            _ => None,
        });
        assert_eq!(peak, Some(4.5));
    }
}
