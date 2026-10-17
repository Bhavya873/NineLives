//! The Manual controller (SPEC §8, debug only): each of the keys 1–0 fires one nozzle at
//! full throttle, so a person can feel what every thruster does.

use nl_sim::conventions::NOZZLE_COUNT;

use crate::{ControlCtx, Controller};

/// Fires nozzle `i` at throttle 1 while its manual button is held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Manual;

impl Controller for Manual {
    fn reset(&mut self, _seed: u64) {}

    fn act(&mut self, ctx: &ControlCtx) -> [f32; NOZZLE_COUNT] {
        let mut throttles = [0.0; NOZZLE_COUNT];
        for (index, throttle) in throttles.iter_mut().enumerate() {
            if ctx.input.is_manual_nozzle_pressed(index) {
                *throttle = 1.0;
            }
        }
        throttles
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nl_sim::input::{FIRST_MANUAL_NOZZLE_BIT, InputFrame};

    fn pressing_nozzles(nozzles: &[usize]) -> ControlCtx {
        let mut buttons = 0;
        for nozzle in nozzles {
            buttons |= 1 << (FIRST_MANUAL_NOZZLE_BIT + *nozzle as u32);
        }
        ControlCtx {
            input: InputFrame {
                buttons,
                ..InputFrame::default()
            },
            ..ControlCtx::default()
        }
    }

    #[test]
    fn each_held_key_fires_its_own_nozzle_at_full_throttle() {
        let throttles = Manual.act(&pressing_nozzles(&[0, 9]));

        assert_eq!(
            throttles,
            [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn nothing_held_means_nothing_fires() {
        let throttles = Manual.act(&pressing_nozzles(&[]));

        assert_eq!(throttles, [0.0; NOZZLE_COUNT]);
    }
}
