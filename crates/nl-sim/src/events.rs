//! Game events: airborne, upset, save and crash (SPEC §7.9).
//!
//! `EventTracker` watches a few numbers each physics step (wheels down, tilt, spin,
//! impacts) and emits `SimEvent`s when a rule is met. It does no physics itself, so
//! the rules can be tested on plain numbers. After a crash it stays quiet until reset.

use serde::{Deserialize, Serialize};

use crate::car_state::Touchdown;
use crate::conventions::WHEEL_COUNT;
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
    /// The car fell out of the world.
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
    },
    /// The car crashed. In the game it explodes and respawns; in training the episode ends.
    Crash { cause: CrashCause },
    /// The car was put back at the last checkpoint, after a crash or the Reset button.
    Respawn,
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
    landing: Option<Touchdown>,
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
}

impl EventTracker {
    /// True once a crash has happened. Nothing else is reported until `reset`.
    pub fn crashed(&self) -> bool {
        self.crashed
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
            events.push(SimEvent::Crash { cause });
            return;
        }
        self.update_airborne(inputs, dt_s, events);
        self.update_upset_and_save(inputs, config, dt_s, events);
        self.previous_wheels_down = inputs.wheels_down;
    }

    /// The first crash rule that applies this step, if any (SPEC §7.9).
    fn crash_cause(&self, inputs: &EventInputs, config: &EventConfig) -> Option<CrashCause> {
        if inputs.impact_speed_m_s > config.crash_impact_speed_m_s {
            return Some(CrashCause::HardImpact);
        }
        if inputs.roof_touching && inputs.tilt_rad > config.upside_down_tilt_deg.to_radians() {
            return Some(CrashCause::UpsideDown);
        }
        if inputs.height_m < config.out_of_bounds_y_m {
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

    /// Airborne starts when no wheel touches, and ends when any wheel touches again.
    fn update_airborne(&mut self, inputs: &EventInputs, dt_s: f32, events: &mut Vec<SimEvent>) {
        let airborne_now = inputs.wheels_down == 0;
        if airborne_now && !self.is_airborne() {
            events.push(SimEvent::AirborneStart);
        }
        if !airborne_now && self.is_airborne() {
            events.push(SimEvent::AirborneEnd);
        }
        if airborne_now {
            self.airborne_s += dt_s;
        } else {
            self.airborne_s = 0.0;
        }
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
        let touched_down = self.previous_wheels_down == 0 && inputs.wheels_down > 0;
        let Some(upset) = self.upset.as_mut() else {
            return;
        };
        upset.elapsed_s += dt_s;
        upset.peak_tilt_rad = upset.peak_tilt_rad.max(inputs.tilt_rad);

        let back_on_wheels = inputs.wheels_down == WHEEL_COUNT
            && inputs.tilt_rad < config.save_tilt_deg.to_radians()
            && inputs.spin_rad_s < config.save_max_spin_rad_s;
        let all_wheels_back = inputs.wheels_down == WHEEL_COUNT;
        if upset.landing.is_none() && (touched_down || all_wheels_back) {
            upset.landing = Some(inputs.motion_before_step);
        }
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
                landing: upset.landing.unwrap_or(inputs.motion_before_step),
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
}
