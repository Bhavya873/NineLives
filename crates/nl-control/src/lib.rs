//! Controllers that turn an observation into 10 nozzle throttles.
//!
//! M4 adds the `Controller` trait (SPEC §8), the Off, Manual and PD
//! controllers, and the thruster allocation. For now this crate only checks
//! that it links against `nl-sim`.

use nl_sim::conventions::NOZZLE_COUNT;

/// Throttles with every nozzle closed. This is what the Off controller returns.
pub fn all_nozzles_off() -> [f32; NOZZLE_COUNT] {
    [0.0; NOZZLE_COUNT]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_nozzles_off_closes_every_nozzle() {
        let throttles = all_nozzles_off();

        assert!(throttles.iter().all(|&throttle| throttle == 0.0));
    }
}
