//! The compressed-air tank that feeds the thrusters (SPEC §7.5).
//!
//! Thrust uses air: mass flow = force / exhaust speed. The tank refills slowly while
//! the car stands on its wheels. Near empty, thrust fades out smoothly instead of
//! cutting off. In unlimited-air modes the tank never drains but air used is counted.

use serde::{Deserialize, Serialize};

use crate::modes::ModeParams;

/// How much air is left and how much has been used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirTank {
    air_kg: f32,
    air_used_kg: f32,
}

impl AirTank {
    /// A full tank for this mode.
    pub fn full(mode: &ModeParams) -> AirTank {
        AirTank {
            air_kg: mode.tank_capacity_kg,
            air_used_kg: 0.0,
        }
    }

    /// Air left in the tank.
    pub fn air_kg(&self) -> f32 {
        self.air_kg
    }

    /// All the air used since the tank was created (for stats and rewards).
    pub fn air_used_kg(&self) -> f32 {
        self.air_used_kg
    }

    /// Air left as a fraction of capacity; always 1 when air is unlimited.
    pub fn air_fraction(&self, mode: &ModeParams) -> f32 {
        if mode.unlimited_air {
            return 1.0;
        }
        (self.air_kg / mode.tank_capacity_kg).clamp(0.0, 1.0)
    }

    /// Multiplier on all thrust (SPEC §7.5): 1 normally, sliding linearly to 0 over the
    /// last `empty_fade_fraction` of the tank, so thrust never stops with a jolt.
    pub fn thrust_gate(&self, mode: &ModeParams) -> f32 {
        if mode.unlimited_air {
            return 1.0;
        }
        let fade_start_kg = mode.empty_fade_fraction * mode.tank_capacity_kg;
        (self.air_kg / fade_start_kg).clamp(0.0, 1.0)
    }

    /// Takes out the air used by `total_force_n` of thrust for `dt_s` seconds.
    /// SPEC §7.5: mass flow = force / exhaust speed.
    pub fn use_air(
        &mut self,
        total_force_n: f32,
        exhaust_speed_m_s: f32,
        dt_s: f32,
        mode: &ModeParams,
    ) {
        let used_kg = total_force_n / exhaust_speed_m_s * dt_s;
        self.air_used_kg += used_kg;
        if !mode.unlimited_air {
            self.air_kg = (self.air_kg - used_kg).max(0.0);
        }
    }

    /// Refills at `refill_kg_s` while enough wheels touch the ground (SPEC §7.5).
    pub fn refill(&mut self, wheels_touching: usize, dt_s: f32, mode: &ModeParams) {
        if wheels_touching >= mode.refill_min_wheels {
            self.air_kg = (self.air_kg + mode.refill_kg_s * dt_s).min(mode.tank_capacity_kg);
        }
    }

    /// Sets the air left, e.g. for training scenarios. Clamped to 0..capacity.
    pub fn set_air_kg(&mut self, air_kg: f32, mode: &ModeParams) {
        self.air_kg = air_kg.clamp(0.0, mode.tank_capacity_kg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modes::{DEFAULT_MODES_TOML, ModesConfig};

    fn realistic() -> ModeParams {
        ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
            .unwrap()
            .realistic
    }

    #[test]
    fn one_second_of_full_lift_uses_80_kg() {
        let mode = realistic();
        let mut tank = AirTank::full(&mode);

        // SPEC §7.5: 48 kN / 600 m/s = 80 kg/s.
        for _ in 0..120 {
            tank.use_air(48_000.0, 600.0, 1.0 / 120.0, &mode);
        }

        assert!(
            (tank.air_used_kg() - 80.0).abs() < 0.8,
            "used {}",
            tank.air_used_kg()
        );
        assert!((tank.air_kg() - 180.0).abs() < 0.8);
    }

    #[test]
    fn thrust_fades_smoothly_over_the_last_two_percent() {
        let mode = realistic();
        let mut tank = AirTank::full(&mode);
        let fade_start_kg = 0.02 * 260.0;

        tank.set_air_kg(fade_start_kg, &mode);
        let at_fade_start = tank.thrust_gate(&mode);
        tank.set_air_kg(fade_start_kg / 2.0, &mode);
        let halfway = tank.thrust_gate(&mode);
        tank.set_air_kg(0.0, &mode);
        let empty = tank.thrust_gate(&mode);

        assert_eq!(at_fade_start, 1.0);
        assert!((halfway - 0.5).abs() < 1e-5);
        assert_eq!(empty, 0.0);
    }

    #[test]
    fn the_tank_refills_only_with_at_least_three_wheels_down() {
        let mode = realistic();
        let mut tank = AirTank::full(&mode);
        tank.set_air_kg(100.0, &mode);

        tank.refill(2, 1.0, &mode);
        let after_two_wheels = tank.air_kg();
        tank.refill(3, 1.0, &mode);
        let after_three_wheels = tank.air_kg();

        assert_eq!(after_two_wheels, 100.0);
        assert_eq!(after_three_wheels, 105.0);
    }

    #[test]
    fn unlimited_air_never_drains_but_counts_air_used() {
        let mut mode = realistic();
        mode.unlimited_air = true;
        let mut tank = AirTank::full(&mode);

        tank.use_air(48_000.0, 600.0, 10.0, &mode);

        assert_eq!(tank.air_kg(), mode.tank_capacity_kg);
        assert_eq!(tank.thrust_gate(&mode), 1.0);
        assert!((tank.air_used_kg() - 800.0).abs() < 1e-3);
    }
}
