//! Chaos: a random knock to test the controller (SPEC §7.12).
//!
//! One push (linear impulse) and one twist (angular impulse), each in a random
//! direction with a random size up to the limits in `[chaos]` of vehicle.toml.
//! Only the world's seeded ChaCha8 generator is used, so runs stay deterministic.

use rand::RngExt;
use rand_chacha::ChaCha8Rng;
use rapier3d::prelude::*;

use crate::vehicle_config::ChaosConfig;

/// Applies one random knock to `body`.
pub fn apply_chaos_knock(body: &mut RigidBody, config: &ChaosConfig, rng: &mut ChaCha8Rng) {
    let push_n_s = random_direction(rng) * rng.random_range(0.0..=config.max_linear_impulse_n_s);
    let twist_n_m_s =
        random_direction(rng) * rng.random_range(0.0..=config.max_angular_impulse_n_m_s);
    body.apply_impulse(push_n_s, true);
    body.apply_torque_impulse(twist_n_m_s, true);
}

/// A random unit vector, evenly spread over all directions.
/// Picks points in a cube and keeps the first one inside the unit ball.
pub(crate) fn random_direction(rng: &mut ChaCha8Rng) -> Vector {
    loop {
        let candidate = Vector::new(
            rng.random_range(-1.0..=1.0),
            rng.random_range(-1.0..=1.0),
            rng.random_range(-1.0..=1.0),
        );
        let length = candidate.length();
        if length > 0.1 && length <= 1.0 {
            return candidate / length;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn random_directions_have_length_one_and_repeat_for_the_same_seed() {
        let mut first = ChaCha8Rng::seed_from_u64(7);
        let mut second = ChaCha8Rng::seed_from_u64(7);

        for _ in 0..100 {
            let direction = random_direction(&mut first);
            assert!((direction.length() - 1.0).abs() < 1e-5);
            assert_eq!(direction, random_direction(&mut second));
        }
    }
}
