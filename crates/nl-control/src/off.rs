//! The Off controller (SPEC §8): every nozzle closed. Hop and Boost still work, because
//! the sim adds their feed-forward on top of whatever the controller asks for.

use nl_sim::conventions::NOZZLE_COUNT;

use crate::{ControlCtx, Controller};

/// Never fires a nozzle.
#[derive(Debug, Clone, Copy, Default)]
pub struct Off;

impl Controller for Off {
    fn reset(&mut self, _seed: u64) {}

    fn act(&mut self, _ctx: &ControlCtx) -> [f32; NOZZLE_COUNT] {
        [0.0; NOZZLE_COUNT]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nl_sim::input::InputFrame;

    #[test]
    fn off_closes_every_nozzle_whatever_the_driver_presses() {
        let ctx = ControlCtx {
            input: InputFrame {
                buttons: u16::MAX,
                ..InputFrame::default()
            },
        };

        let throttles = Off.act(&ctx);

        assert_eq!(throttles, [0.0; NOZZLE_COUNT]);
    }
}
