//! What the driver (a person, the driver bot or a test script) does on one physics step.
//!
//! An `InputFrame` is tiny on purpose: the browser sends one to wasm every tick, and the
//! ghost recorder stores one per tick (SPEC §11.1). It packs into 4 bytes.

use serde::{Deserialize, Serialize};

use crate::conventions::NOZZLE_COUNT;

/// Button bit: Hop.
pub const BUTTON_HOP: u16 = 1 << 0;
/// Button bit: Boost.
pub const BUTTON_BOOST: u16 = 1 << 1;
/// Button bit: Chaos (a random knock).
pub const BUTTON_CHAOS: u16 = 1 << 2;
/// Button bit: reset to the last checkpoint.
pub const BUTTON_RESET: u16 = 1 << 3;
/// Bit of the first manual nozzle button. Nozzle `i` uses bit `FIRST_MANUAL_NOZZLE_BIT + i`.
pub const FIRST_MANUAL_NOZZLE_BIT: u16 = 4;

/// Number of bytes in a packed `InputFrame`.
pub const INPUT_FRAME_BYTES: usize = 4;

/// The driver's controls for one physics step.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputFrame {
    /// Throttle from -127 (full brake or reverse) to 127 (full throttle).
    pub throttle: i8,
    /// Steering from -127 (full right) to 127 (full left). Positive is left so it matches
    /// the yaw sign: positive yaw turns the nose left (SPEC §4).
    pub steer: i8,
    /// Pressed buttons, one bit each (`BUTTON_*` and the manual nozzle bits).
    pub buttons: u16,
}

impl InputFrame {
    /// Throttle as a fraction from -1.0 to 1.0.
    pub fn throttle_fraction(&self) -> f32 {
        to_fraction(self.throttle)
    }

    /// Steering as a fraction from -1.0 (right) to 1.0 (left).
    pub fn steer_fraction(&self) -> f32 {
        to_fraction(self.steer)
    }

    /// True if every bit in `button` is pressed. An empty mask (0) is never pressed.
    pub fn is_pressed(&self, button: u16) -> bool {
        button != 0 && self.buttons & button == button
    }

    /// True if the manual button for nozzle `nozzle_index` (0..10) is pressed.
    pub fn is_manual_nozzle_pressed(&self, nozzle_index: usize) -> bool {
        if nozzle_index >= NOZZLE_COUNT {
            return false;
        }
        self.is_pressed(1 << (FIRST_MANUAL_NOZZLE_BIT as usize + nozzle_index))
    }

    /// Packs the frame into 4 bytes: throttle, steer, buttons (little-endian).
    pub fn to_bytes(&self) -> [u8; INPUT_FRAME_BYTES] {
        let [buttons_low, buttons_high] = self.buttons.to_le_bytes();
        [
            self.throttle as u8,
            self.steer as u8,
            buttons_low,
            buttons_high,
        ]
    }

    /// Unpacks a frame made by `to_bytes`.
    pub fn from_bytes(bytes: [u8; INPUT_FRAME_BYTES]) -> Self {
        InputFrame {
            throttle: bytes[0] as i8,
            steer: bytes[1] as i8,
            buttons: u16::from_le_bytes([bytes[2], bytes[3]]),
        }
    }
}

/// Maps -128..=127 to -1.0..=1.0. -128 is treated like -127 so both ends are symmetric.
fn to_fraction(value: i8) -> f32 {
    (value as f32 / 127.0).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_throttle_is_one() {
        let input = InputFrame {
            throttle: 127,
            ..InputFrame::default()
        };

        assert_eq!(input.throttle_fraction(), 1.0);
    }

    #[test]
    fn most_negative_value_is_minus_one_not_less() {
        let input = InputFrame {
            steer: -128,
            ..InputFrame::default()
        };

        assert_eq!(input.steer_fraction(), -1.0);
    }

    #[test]
    fn packing_and_unpacking_gives_the_same_frame() {
        let input = InputFrame {
            throttle: -40,
            steer: 99,
            buttons: BUTTON_HOP | BUTTON_RESET | (1 << 13),
        };

        let round_trip = InputFrame::from_bytes(input.to_bytes());

        assert_eq!(round_trip, input);
    }

    #[test]
    fn manual_nozzle_buttons_map_to_their_own_bits() {
        let input = InputFrame {
            buttons: 1 << (FIRST_MANUAL_NOZZLE_BIT + 9),
            ..InputFrame::default()
        };

        assert!(input.is_manual_nozzle_pressed(9));
        assert!(!input.is_manual_nozzle_pressed(0));
        assert!(!input.is_pressed(BUTTON_HOP));
    }

    #[test]
    fn an_empty_button_mask_is_never_pressed() {
        let input = InputFrame {
            buttons: u16::MAX,
            ..InputFrame::default()
        };

        assert!(!input.is_pressed(0));
    }
}
