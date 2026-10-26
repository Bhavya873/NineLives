//! Hop and Boost: the driver's two thruster buttons (SPEC §7.6–7.8).
//!
//! The game adds their throttles ("feed-forward") on top of the controller's:
//! Hop puts `hop_ff` on the four lift nozzles for `hop_s`; Boost puts `boost_ff` on the
//! two boost nozzles while held. `combine_throttles` then adds and clamps (SPEC §7.7).

use serde::{Deserialize, Serialize};

use crate::conventions::{NOZZLE_COUNT, ON_WHEELS_MIN_WHEELS};
use crate::modes::ModeParams;
use crate::thrusters::safe_throttle;
use crate::vehicle_config::DriveConfig;

/// Indices of the four lift nozzles (L_FL, L_FR, L_RL, L_RR).
const LIFT_NOZZLES: [usize; 4] = [0, 1, 2, 3];
/// Indices of the two boost nozzles (B_L, B_R).
const BOOST_NOZZLES: [usize; 2] = [8, 9];

/// Why the driver cannot Hop right now. The HUD shows it (SPEC §7.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HopBlockedReason {
    /// Fewer than 3 wheels touch the ground.
    NotOnGround,
    /// The last Hop was too recent.
    Cooldown,
    /// Less air than `hop_min_air_kg` (Realistic only).
    NoAir,
}

/// The state of the Hop and Boost buttons' effects.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HopBoost {
    hop_time_left_s: f32,
    hop_cooldown_left_s: f32,
    boost_active: bool,
}

impl HopBoost {
    /// Advances the timers by `dt_s` and reacts to the buttons. A Hop starts when
    /// `hop_just_pressed` (the Hop button went down this step) and nothing blocks it;
    /// Boost runs while `boost_held`.
    pub fn update(
        &mut self,
        hop_just_pressed: bool,
        boost_held: bool,
        wheels_touching: usize,
        air_kg: f32,
        mode: &ModeParams,
        dt_s: f32,
    ) {
        self.hop_time_left_s = (self.hop_time_left_s - dt_s).max(0.0);
        self.hop_cooldown_left_s = (self.hop_cooldown_left_s - dt_s).max(0.0);

        let blocked = self.hop_blocked_reason(wheels_touching, air_kg, mode);
        if hop_just_pressed && blocked.is_none() {
            self.hop_time_left_s = mode.hop_s;
            self.hop_cooldown_left_s = mode.hop_cooldown_s;
        }

        self.boost_active = boost_held;
    }

    /// Why a Hop would be refused right now, or `None` if it is allowed (SPEC §7.8).
    pub fn hop_blocked_reason(
        &self,
        wheels_touching: usize,
        air_kg: f32,
        mode: &ModeParams,
    ) -> Option<HopBlockedReason> {
        if wheels_touching < ON_WHEELS_MIN_WHEELS {
            return Some(HopBlockedReason::NotOnGround);
        }
        if self.hop_cooldown_left_s > 0.0 {
            return Some(HopBlockedReason::Cooldown);
        }
        if !mode.unlimited_air && air_kg < mode.hop_min_air_kg {
            return Some(HopBlockedReason::NoAir);
        }
        None
    }

    /// The throttles Hop and Boost add this step, in nozzle order. `boost_share` (0..1,
    /// from `boost_speed_share`) scales Boost down near the Boost speed limit.
    pub fn feed_forward_throttles(
        &self,
        mode: &ModeParams,
        boost_share: f32,
    ) -> [f32; NOZZLE_COUNT] {
        let mut throttles = [0.0; NOZZLE_COUNT];
        if self.hop_active() {
            for nozzle in LIFT_NOZZLES {
                throttles[nozzle] = mode.hop_ff;
            }
        }
        if self.boost_active {
            for nozzle in BOOST_NOZZLES {
                throttles[nozzle] = mode.boost_ff * boost_share;
            }
        }
        throttles
    }

    /// True while a Hop is pushing.
    pub fn hop_active(&self) -> bool {
        self.hop_time_left_s > 0.0
    }

    /// Seconds of push left in the current Hop.
    pub fn hop_time_left_s(&self) -> f32 {
        self.hop_time_left_s
    }

    /// Seconds until the next Hop is allowed.
    pub fn hop_cooldown_left_s(&self) -> f32 {
        self.hop_cooldown_left_s
    }

    /// True while the Boost button is held.
    pub fn boost_active(&self) -> bool {
        self.boost_active
    }
}

/// SPEC §7.7: `u_cmd = clamp(u_ff + u_ctrl, 0, 1)`. During a Hop the controller can
/// only add lift on top of `hop_ff`, never take it away.
pub fn combine_throttles(
    feed_forward: [f32; NOZZLE_COUNT],
    controller: [f32; NOZZLE_COUNT],
) -> [f32; NOZZLE_COUNT] {
    let mut commanded = [0.0; NOZZLE_COUNT];
    for index in 0..NOZZLE_COUNT {
        // A NaN from the controller counts as 0, the same rule as the valves.
        let controller_throttle = safe_throttle(controller[index]);
        commanded[index] = (feed_forward[index] + controller_throttle).clamp(0.0, 1.0);
    }
    commanded
}

/// How much of the Boost throttle is left at `forward_speed_m_s` (SPEC §7.8): all of it
/// below the fade band, fading linearly to none at `drive.boost_max_speed_m_s`.
pub fn boost_speed_share(drive: &DriveConfig, forward_speed_m_s: f32) -> f32 {
    let speed_left_m_s = drive.boost_max_speed_m_s - forward_speed_m_s;
    (speed_left_m_s / drive.boost_fade_m_s).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modes::{DEFAULT_MODES_TOML, ModesConfig};

    const DT_S: f32 = 1.0 / 120.0;

    fn realistic() -> ModeParams {
        ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
            .unwrap()
            .realistic
    }

    #[test]
    fn a_hop_puts_hop_ff_on_the_lift_nozzles_only() {
        let mode = realistic();
        let mut hop_boost = HopBoost::default();

        hop_boost.update(true, false, 4, 260.0, &mode, DT_S);

        let throttles = hop_boost.feed_forward_throttles(&mode, 1.0);
        assert_eq!(&throttles[0..4], &[0.75; 4]);
        assert_eq!(&throttles[4..10], &[0.0; 6]);
    }

    #[test]
    fn no_hop_starts_without_a_press() {
        let mode = realistic();
        let mut hop_boost = HopBoost::default();

        hop_boost.update(false, false, 4, 260.0, &mode, DT_S);

        assert!(!hop_boost.hop_active());
    }

    #[test]
    fn hop_is_blocked_in_the_air_during_cooldown_and_without_air() {
        let mode = realistic();
        let mut hop_boost = HopBoost::default();

        assert_eq!(
            hop_boost.hop_blocked_reason(2, 260.0, &mode),
            Some(HopBlockedReason::NotOnGround)
        );
        assert_eq!(
            hop_boost.hop_blocked_reason(4, 10.0, &mode),
            Some(HopBlockedReason::NoAir)
        );
        hop_boost.update(true, false, 4, 260.0, &mode, DT_S);
        assert_eq!(
            hop_boost.hop_blocked_reason(4, 260.0, &mode),
            Some(HopBlockedReason::Cooldown)
        );
    }

    #[test]
    fn the_controller_cannot_take_lift_away_during_a_hop() {
        let feed_forward = [0.75, 0.75, 0.75, 0.75, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0];
        let controller = [0.5, 0.0, f32::NAN, 0.1, 0.3, 0.0, 0.0, 0.0, 0.0, 0.0];

        let commanded = combine_throttles(feed_forward, controller);

        assert_eq!(
            commanded,
            [1.0, 0.75, 0.75, 0.85, 0.3, 0.0, 0.0, 0.0, 1.0, 1.0]
        );
    }

    fn drive_config() -> DriveConfig {
        crate::vehicle_config::VehicleConfig::from_toml_str(
            crate::vehicle_config::DEFAULT_VEHICLE_TOML,
        )
        .unwrap()
        .drive
    }

    #[test]
    fn boost_is_full_below_the_fade_band_and_gone_at_the_limit() {
        let drive = drive_config();
        let limit_m_s = drive.boost_max_speed_m_s;
        assert_eq!(boost_speed_share(&drive, 0.0), 1.0);
        assert_eq!(
            boost_speed_share(&drive, limit_m_s - drive.boost_fade_m_s),
            1.0
        );
        assert_eq!(boost_speed_share(&drive, limit_m_s), 0.0);
        assert_eq!(boost_speed_share(&drive, limit_m_s + 50.0), 0.0);
    }

    #[test]
    fn boost_is_half_in_the_middle_of_the_fade_band() {
        let drive = drive_config();
        let middle_m_s = drive.boost_max_speed_m_s - 0.5 * drive.boost_fade_m_s;
        assert!((boost_speed_share(&drive, middle_m_s) - 0.5).abs() < 1e-6);
    }
}
