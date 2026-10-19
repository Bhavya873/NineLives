//! A prediction of how high a Hop lifts the car over time (SPEC §7.6, §7.8), for the
//! driver bot's timing and Stage C's car-row placement. It is a one-dimensional
//! estimate on flat ground with the controller off: the four lift nozzles at `hop_ff`
//! for `hop_s` through the valve lag, against gravity. The real sim (springs, pitch,
//! controller help) differs a little: the springs' push at lift-off adds about 0.3 m in
//! Realistic, so the prediction errs low, and a test keeps it below and close to the sim.

use crate::conventions::{GRAVITY_M_S2, PHYSICS_DT_S};
use crate::modes::ModeParams;
use crate::vehicle_config::VehicleConfig;

/// The four lift nozzles are the first four (SPEC §7.4 order).
const LIFT_NOZZLES: usize = 4;
/// Longest flight the prediction follows, in s.
const MAX_FLIGHT_S: f32 = 6.0;

/// When the car is at least a given height above where it started, in s after the
/// Hop press.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HopWindow {
    /// The car first rises past the height, in s.
    pub from_s: f32,
    /// It falls back below it, in s.
    pub until_s: f32,
}

impl HopWindow {
    /// The middle of the window, in s after the press.
    pub fn middle_s(&self) -> f32 {
        (self.from_s + self.until_s) / 2.0
    }
}

/// The predicted rise of the car after a Hop press, worked out once and then asked
/// about (the bot and Stage C ask several questions per episode).
#[derive(Debug, Clone, PartialEq)]
pub struct HopProfile {
    /// Rise above the resting height, one value per physics step (index 0 is the
    /// press), until the car is back down, in m.
    rise_m: Vec<f32>,
}

impl HopProfile {
    /// The prediction for this car and mode.
    pub fn new(vehicle: &VehicleConfig, mode: &ModeParams) -> HopProfile {
        HopProfile {
            rise_m: hop_rise_profile_m(vehicle, mode),
        }
    }

    /// When the hop keeps the car at least `rise_m` above its resting height; None if
    /// it never gets that high.
    pub fn clear_window(&self, rise_m: f32) -> Option<HopWindow> {
        let mut from_s = None;
        for (step, height_m) in self.rise_m.iter().enumerate() {
            let time_s = step as f32 * PHYSICS_DT_S;
            match from_s {
                None if *height_m >= rise_m => from_s = Some(time_s),
                Some(from_s) if *height_m < rise_m => {
                    return Some(HopWindow {
                        from_s,
                        until_s: time_s,
                    });
                }
                _ => {}
            }
        }
        None
    }

    /// The highest point of the hop, in m above the resting height.
    pub fn apex_m(&self) -> f32 {
        let mut apex_m: f32 = 0.0;
        for height_m in &self.rise_m {
            apex_m = apex_m.max(*height_m);
        }
        apex_m
    }

    /// When the hop is highest, in s after the press.
    pub fn apex_time_s(&self) -> f32 {
        let mut apex_m = 0.0;
        let mut apex_step = 0;
        for (step, height_m) in self.rise_m.iter().enumerate() {
            if *height_m > apex_m {
                apex_m = *height_m;
                apex_step = step;
            }
        }
        apex_step as f32 * PHYSICS_DT_S
    }

    /// The best moment to be over the middle of something `rise_m` tall: the middle of
    /// the clear window, or the apex if the hop never gets that high, in s.
    pub fn lead_time_s(&self, rise_m: f32) -> f32 {
        match self.clear_window(rise_m) {
            Some(window) => window.middle_s(),
            None => self.apex_time_s(),
        }
    }
}

/// The predicted rise of the car above its resting height after a Hop press, one value
/// per physics step (index 0 is the press), until it is back down, in m.
fn hop_rise_profile_m(vehicle: &VehicleConfig, mode: &ModeParams) -> Vec<f32> {
    let mut lift_n = 0.0;
    for nozzle in vehicle.thrusters.nozzles.iter().take(LIFT_NOZZLES) {
        lift_n += nozzle.max_force_n * nozzle.direction[1] * mode.thrust_scale;
    }
    let mass_kg = vehicle.chassis.dry_mass_kg;
    // SPEC §7.4 valve lag: the throttle moves this fraction of the way each step.
    let lag_fraction =
        1.0 - libm::expf(-PHYSICS_DT_S / vehicle.thrusters.valve_time_constant_s.max(1e-6));
    let mut throttle = 0.0;
    let mut speed_m_s = 0.0;
    let mut rise_m: f32 = 0.0;
    let mut profile = vec![0.0];
    let steps = (MAX_FLIGHT_S / PHYSICS_DT_S) as usize;
    for step in 1..=steps {
        let time_s = step as f32 * PHYSICS_DT_S;
        let commanded = if time_s <= mode.hop_s {
            mode.hop_ff
        } else {
            0.0
        };
        throttle += (commanded - throttle) * lag_fraction;
        let accel_m_s2 = throttle * lift_n / mass_kg - GRAVITY_M_S2;
        speed_m_s += accel_m_s2 * PHYSICS_DT_S;
        rise_m += speed_m_s * PHYSICS_DT_S;
        if rise_m <= 0.0 {
            // The ground holds the car up until the thrust beats gravity.
            if time_s > mode.hop_s {
                break;
            }
            rise_m = 0.0;
            speed_m_s = 0.0;
        }
        profile.push(rise_m);
    }
    profile
}
