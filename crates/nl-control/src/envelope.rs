//! The policy envelope (plan G3, docs/contracts/events-hud.md field 27): where the trained
//! policy has been validated. The policy was only judged up to 55 m/s, and its observation
//! clamps velocity at 60 m/s, so outside the envelope the AI is not trusted and the Hybrid
//! hands the nozzles to PD. The game, the wasm `Game`, the viewer crowd and the Rust eval
//! all decide through this one function (`HybridGate` calls it).
//!
//! "Validated" means someone ran the Hybrid eval in that region and logged the result in
//! docs/EXPERIMENTS.md; only then is `[hybrid] policy_max_speed_m_s` raised in
//! controllers.toml. Nothing above 55 m/s has been validated yet.

use nl_sim::car_state::CarState;
use nl_sim::obs::{OBS_SPEC, Observation};

/// The default validated speed limit, in m/s (the speed the policy was evaluated up to).
pub const DEFAULT_POLICY_MAX_SPEED_M_S: f32 = 55.0;

/// Observation fields whose range bound is a saturation limit: a value on the bound means
/// the real quantity was at or past it, so the policy sees a wrong number. Angular velocity
/// (3..6), body velocity (6..9), `thrust_scale` (19) and `hop_time_left` (22).
///
/// The other fields are left out on purpose, because they sit on a bound in normal driving:
/// flags and fractions (0 or 1), `height` and the terrain probes (capped on purpose, "far
/// from the ground" is a valid input), the direction vectors and the driver and
/// heading inputs (±1 is a legal value).
const SATURATING_FIELDS: [usize; 8] = [3, 4, 5, 6, 7, 8, 19, 22];

/// Where the policy may be trusted, from controllers.toml `[hybrid]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolicyEnvelope {
    /// Outside the envelope above this speed, in m/s (compared with the size of the
    /// body-frame forward speed, so reversing counts too).
    pub max_speed_m_s: f32,
}

impl Default for PolicyEnvelope {
    fn default() -> PolicyEnvelope {
        PolicyEnvelope {
            max_speed_m_s: DEFAULT_POLICY_MAX_SPEED_M_S,
        }
    }
}

impl PolicyEnvelope {
    /// True when the AI is outside where it was validated: faster than `max_speed_m_s`, or
    /// an observation value sits on its clamp bound (see `SATURATING_FIELDS`).
    pub fn is_outside(&self, state: &CarState, observation: &Observation) -> bool {
        state.linear_velocity_body_m_s[0].abs() > self.max_speed_m_s
            || observation_is_clamped(observation)
    }
}

/// True when a saturating observation field sits on a bound of its `OBS_SPEC` range. A
/// lower bound of 0 is where `thrust_scale` and `hop_time_left` rest, so only the signed
/// fields (velocities) count at their low end.
pub fn observation_is_clamped(observation: &Observation) -> bool {
    SATURATING_FIELDS.iter().any(|&index| {
        let field = &OBS_SPEC[index];
        (field.low < 0.0 && observation[index] <= field.low) || observation[index] >= field.high
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nl_sim::obs::OBS_DIM;

    fn with_forward_speed(speed: f32) -> CarState {
        CarState {
            linear_velocity_body_m_s: [speed, 0.0, 0.0],
            ..CarState::default()
        }
    }

    #[test]
    fn the_saturating_fields_are_the_ones_named_in_the_docs() {
        let names: Vec<&str> = SATURATING_FIELDS
            .iter()
            .map(|&i| OBS_SPEC[i].name)
            .collect();

        assert_eq!(
            names,
            [
                "ang_vel_body.x",
                "ang_vel_body.y",
                "ang_vel_body.z",
                "lin_vel_body.x",
                "lin_vel_body.y",
                "lin_vel_body.z",
                "thrust_scale",
                "hop_time_left"
            ]
        );
    }

    #[test]
    fn speed_past_the_limit_is_outside_and_the_limit_itself_is_not() {
        let envelope = PolicyEnvelope::default();
        let observation = [0.0; OBS_DIM];

        assert!(!envelope.is_outside(&with_forward_speed(55.0), &observation));
        assert!(envelope.is_outside(&with_forward_speed(55.01), &observation));
        assert!(envelope.is_outside(&with_forward_speed(-60.0), &observation));
    }

    #[test]
    fn a_raised_limit_moves_the_envelope() {
        let envelope = PolicyEnvelope {
            max_speed_m_s: 80.0,
        };

        assert!(!envelope.is_outside(&with_forward_speed(70.0), &[0.0; OBS_DIM]));
        assert!(envelope.is_outside(&with_forward_speed(81.0), &[0.0; OBS_DIM]));
    }

    #[test]
    fn a_clamped_observation_is_outside_at_any_speed() {
        let envelope = PolicyEnvelope::default();
        for &index in &SATURATING_FIELDS {
            let low = OBS_SPEC[index].low;
            let bounds = if low < 0.0 {
                vec![low, OBS_SPEC[index].high]
            } else {
                vec![OBS_SPEC[index].high]
            };
            for bound in bounds {
                let mut observation = [0.0; OBS_DIM];
                observation[index] = bound;
                assert!(
                    envelope.is_outside(&with_forward_speed(10.0), &observation),
                    "{} at {bound}",
                    OBS_SPEC[index].name
                );
            }
        }
    }

    #[test]
    fn fields_that_sit_on_a_bound_in_normal_driving_do_not_count() {
        let envelope = PolicyEnvelope::default();
        // Every field not listed as saturating, put on both its bounds one at a time.
        for (index, field) in OBS_SPEC.iter().enumerate() {
            if SATURATING_FIELDS.contains(&index) {
                continue;
            }
            for bound in [field.low, field.high] {
                let mut observation = [0.0; OBS_DIM];
                observation[index] = bound;
                assert!(
                    !envelope.is_outside(&with_forward_speed(10.0), &observation),
                    "{} at {bound}",
                    field.name
                );
            }
        }
    }
}
