//! Landing quality (PLAN §3.3, SPEC §7.9): how far the nose turned between take-off and
//! touchdown, and how much forward speed a landing kept. `EventTracker` records the
//! take-off and judges each upset's landing with these, and the Save event carries the
//! result to the reward's save quality and the eval rows. Plain numbers in and out, so
//! the rules can be tested without physics.

use serde::{Deserialize, Serialize};

use crate::car_state::Touchdown;
use crate::vehicle_config::EventConfig;

/// A flattened nose shorter than this has no clear heading (the nose points almost
/// straight up or down).
const MIN_FLAT_NOSE_LENGTH: f32 = 1e-3;

/// The car at its latest take-off (the last AirborneStart).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TakeOff {
    /// The nose flattened onto the ground, as a unit vector [world x, world z]; None
    /// when the nose pointed almost straight up or down.
    pub heading_xz: Option<[f32; 2]>,
    /// Forward speed (along the nose) as the wheels left the ground, in m/s; None when
    /// the car was not on its wheels before, e.g. an episode that starts in the air.
    pub forward_speed_m_s: Option<f32>,
}

impl TakeOff {
    /// A take-off with the nose along `nose_world` (any length, world frame) and
    /// `forward_speed_m_s` if the car left the ground from its wheels.
    pub fn new(nose_world: [f32; 3], forward_speed_m_s: Option<f32>) -> TakeOff {
        TakeOff {
            heading_xz: flat_heading(nose_world),
            forward_speed_m_s,
        }
    }
}

/// How well a landing kept the car's heading and speed (PLAN §3.3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct LandingQuality {
    /// Angle between the flattened nose at touchdown and at take-off, 0..π rad; None if
    /// either had no clear heading or the car did not take off from its wheels.
    pub heading_error_rad: Option<f32>,
    /// Forward speed `speed_kept_delay_s` after touchdown ÷ forward speed at take-off;
    /// None unless the car took off from its wheels faster than
    /// `speed_kept_min_take_off_m_s`. Can be above 1 or below 0.
    pub speed_kept: Option<f32>,
}

/// A landing being judged: started at touchdown, its speed kept measured
/// `speed_kept_delay_s` later.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Landing {
    /// How the car was moving just before touchdown.
    pub motion: Touchdown,
    /// What has been measured so far.
    pub quality: LandingQuality,
    /// Forward speed at take-off, when it is large enough to measure speed kept, in m/s.
    take_off_speed_m_s: Option<f32>,
    /// Time since touchdown, in s.
    since_touchdown_s: f32,
}

impl Landing {
    /// Starts judging a touchdown: `motion` just before it, the `take_off` before it,
    /// and the nose (`nose_world`, world frame) now.
    pub fn at_touchdown(
        motion: Touchdown,
        take_off: Option<TakeOff>,
        nose_world: [f32; 3],
        config: &EventConfig,
    ) -> Landing {
        // Only a take-off from the wheels has a heading to keep (PLAN §3.3: the heading
        // at take-off, Stage C/D). A drop that starts in the air in a random attitude
        // has none, and judging it would push one particular way of righting the car.
        let take_off_heading = take_off
            .filter(|take_off| take_off.forward_speed_m_s.is_some())
            .and_then(|take_off| take_off.heading_xz);
        let heading_now = flat_heading(nose_world);
        let heading_error_rad = match (take_off_heading, heading_now) {
            (Some(before), Some(now)) => Some(heading_error_rad(before, now)),
            _ => None,
        };
        let take_off_speed_m_s = take_off
            .and_then(|take_off| take_off.forward_speed_m_s)
            .filter(|speed_m_s| *speed_m_s > config.speed_kept_min_take_off_m_s);
        Landing {
            motion,
            quality: LandingQuality {
                heading_error_rad,
                speed_kept: None,
            },
            take_off_speed_m_s,
            since_touchdown_s: 0.0,
        }
    }

    /// Advances the landing's clock by `dt_s`; once `speed_kept_delay_s` has passed,
    /// measures the speed kept from `forward_speed_m_s` (once).
    pub fn update(&mut self, forward_speed_m_s: f32, config: &EventConfig, dt_s: f32) {
        self.since_touchdown_s += dt_s;
        if self.since_touchdown_s >= config.speed_kept_delay_s {
            self.measure_speed_kept(forward_speed_m_s);
        }
    }

    /// The quality so far; if the speed kept is due but not yet measured (a save came
    /// before the delay ran out), it is measured now from `forward_speed_m_s`.
    pub fn finish(&mut self, forward_speed_m_s: f32) -> LandingQuality {
        self.measure_speed_kept(forward_speed_m_s);
        self.quality
    }

    /// Fills in the speed kept, unless it is already known or not measurable.
    fn measure_speed_kept(&mut self, forward_speed_m_s: f32) {
        if self.quality.speed_kept.is_some() {
            return;
        }
        if let Some(take_off_speed_m_s) = self.take_off_speed_m_s {
            self.quality.speed_kept = Some(forward_speed_m_s / take_off_speed_m_s);
        }
    }
}

/// The nose direction `nose_world` (world frame) flattened onto the ground, as a unit
/// vector [x, z]; None when it points almost straight up or down.
pub fn flat_heading(nose_world: [f32; 3]) -> Option<[f32; 2]> {
    let [x, _, z] = nose_world;
    let length = libm::sqrtf(x * x + z * z);
    if length < MIN_FLAT_NOSE_LENGTH {
        return None;
    }
    Some([x / length, z / length])
}

/// The unsigned angle between two flat unit headings [x, z], 0..π rad.
pub fn heading_error_rad(before: [f32; 2], now: [f32; 2]) -> f32 {
    let dot = before[0] * now[0] + before[1] * now[1];
    let cross = before[0] * now[1] - before[1] * now[0];
    libm::atan2f(cross.abs(), dot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};

    fn config() -> EventConfig {
        VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML)
            .unwrap()
            .events
    }

    const STILL: Touchdown = Touchdown {
        spin_rad_s: 0.0,
        fall_m_s: 0.0,
    };

    #[test]
    fn landing_turned_a_quarter_turn_has_a_90_degree_heading_error() {
        let take_off = TakeOff::new([1.0, 0.0, 0.0], Some(10.0));

        let landing = Landing::at_touchdown(STILL, Some(take_off), [0.0, 0.0, -1.0], &config());

        let error_deg = landing.quality.heading_error_rad.unwrap().to_degrees();
        assert!((error_deg - 90.0).abs() < 1e-3, "{error_deg}");
    }

    #[test]
    fn heading_ignores_pitch_so_a_nose_down_landing_facing_ahead_has_no_error() {
        let take_off = TakeOff::new([1.0, 0.0, 0.0], Some(10.0));

        let landing = Landing::at_touchdown(STILL, Some(take_off), [0.7, -0.7, 0.0], &config());

        assert!(landing.quality.heading_error_rad.unwrap().abs() < 1e-6);
    }

    #[test]
    fn a_drop_that_starts_in_the_air_has_no_heading_to_keep() {
        let in_the_air = TakeOff::new([1.0, 0.0, 0.0], None);

        let landing = Landing::at_touchdown(STILL, Some(in_the_air), [0.0, 0.0, -1.0], &config());

        assert_eq!(landing.quality.heading_error_rad, None);
    }

    #[test]
    fn a_nose_pointing_straight_down_has_no_heading() {
        assert_eq!(flat_heading([0.0, -1.0, 0.0]), None);
    }

    #[test]
    fn speed_kept_is_measured_half_a_second_after_touchdown() {
        let config = config();
        let take_off = TakeOff::new([1.0, 0.0, 0.0], Some(20.0));
        let mut landing = Landing::at_touchdown(STILL, Some(take_off), [1.0, 0.0, 0.0], &config);

        landing.update(18.0, &config, 0.25);
        let before_delay = landing.quality.speed_kept;
        landing.update(15.0, &config, 0.25);
        landing.update(5.0, &config, 0.25);

        assert_eq!(before_delay, None);
        assert_eq!(landing.quality.speed_kept, Some(0.75));
    }

    #[test]
    fn speed_kept_is_not_measured_after_a_slow_take_off_or_a_start_in_the_air() {
        let config = config();
        let slow = TakeOff::new([1.0, 0.0, 0.0], Some(1.0));
        let in_the_air = TakeOff::new([1.0, 0.0, 0.0], None);

        let mut after_slow = Landing::at_touchdown(STILL, Some(slow), [1.0, 0.0, 0.0], &config);
        let mut after_air =
            Landing::at_touchdown(STILL, Some(in_the_air), [1.0, 0.0, 0.0], &config);

        assert_eq!(after_slow.finish(1.0).speed_kept, None);
        assert_eq!(after_air.finish(10.0).speed_kept, None);
    }
}
