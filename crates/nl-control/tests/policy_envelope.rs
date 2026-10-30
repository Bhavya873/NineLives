//! The policy envelope end to end (plan G3): above 55 m/s, or with a clamped observation,
//! the Hybrid flies with PD's throttles even in an upset the AI would take; inside the
//! envelope nothing changes.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};
use nl_control::{Ai, ControlCtx, Controller, Hybrid, HybridGateSettings, Pd};
use nl_policy::Policy;
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModeParams, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

const UPSIDE_DOWN: [f32; 4] = [1.0, 0.0, 0.0, 0.0];

fn vehicle() -> VehicleConfig {
    VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap()
}

fn mode() -> ModeParams {
    ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
        .unwrap()
        .realistic
}

fn gate(max_speed_m_s: Option<f32>) -> HybridGateSettings {
    let controllers = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();
    HybridGateSettings::for_vehicle(&vehicle())
        .with_enter_tilt(controllers.hybrid.enter_tilt_deg)
        .with_policy_max_speed(max_speed_m_s)
}

fn pd() -> Pd {
    let controllers = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();
    Pd::for_vehicle(&vehicle(), mode(), &controllers)
}

fn hybrid(max_speed_m_s: Option<f32>) -> Hybrid {
    let policy = Policy::from_json(include_str!("../../../models/policy.json")).unwrap();
    Hybrid::new(pd(), Ai::new(Arc::new(policy)), gate(max_speed_m_s))
}

/// A car let go upside down, 30 m up, moving `speed_m_s` forward and spinning `spin`.
fn world(speed_m_s: f32, spin: [f32; 3]) -> World {
    let mut world = World::new_on_flat_ground(&vehicle(), mode(), DEFAULT_GROUND_FRICTION, 0);
    world
        .place_car([0.0, 30.0, 0.0], UPSIDE_DOWN, [speed_m_s, 0.0, 0.0], spin)
        .unwrap();
    world
}

/// Flies `hybrid` next to a PD given the same views for `control_steps` steps. Returns
/// (steps the AI had the nozzles, steps outside the envelope, steps where the Hybrid's
/// throttles differed from PD's).
fn fly(mut world: World, mut hybrid: Hybrid, control_steps: usize) -> (usize, usize, usize) {
    let mut pd = pd();
    hybrid.reset(0);
    pd.reset(0);
    let (mut ai_steps, mut outside_steps, mut differing) = (0, 0, 0);
    let mut done = 0;
    while done < control_steps {
        if world.is_control_step() {
            let ctx = ControlCtx::for_world(&world, InputFrame::default());
            let throttles = hybrid.act(&ctx);
            let pd_throttles = pd.act(&ctx);
            ai_steps += usize::from(hybrid.ai_in_control());
            outside_steps += usize::from(hybrid.policy_out_of_envelope());
            differing += usize::from(throttles != pd_throttles);
            world.set_controller_throttles(throttles);
            done += 1;
        }
        world.step(InputFrame::default());
    }
    (ai_steps, outside_steps, differing)
}

#[test]
fn inside_the_envelope_an_upside_down_car_goes_to_the_ai() {
    let (ai_steps, outside_steps, _) = fly(world(30.0, [0.0; 3]), hybrid(None), 20);

    assert_eq!(ai_steps, 20);
    assert_eq!(outside_steps, 0);
}

#[test]
fn above_55_m_s_pd_flies_the_same_upset() {
    let (ai_steps, outside_steps, differing) = fly(world(70.0, [0.0; 3]), hybrid(None), 20);

    assert_eq!(ai_steps, 0);
    assert_eq!(outside_steps, 20);
    assert_eq!(differing, 0, "the Hybrid sent PD's throttles");
}

#[test]
fn a_clamped_observation_hands_the_nozzles_to_pd_at_low_speed() {
    // 40 rad/s of roll is past the 30 rad/s the observation reports.
    let (ai_steps, outside_steps, differing) = fly(world(10.0, [40.0, 0.0, 0.0]), hybrid(None), 3);

    assert_eq!(ai_steps, 0);
    assert_eq!(outside_steps, 3);
    assert_eq!(differing, 0);
}

#[test]
fn raising_the_validated_limit_gives_the_ai_the_fast_upset_back() {
    let (ai_steps, outside_steps, _) = fly(world(58.0, [0.0; 3]), hybrid(Some(59.0)), 20);

    assert_eq!(ai_steps, 20);
    assert_eq!(outside_steps, 0);
}

#[test]
fn a_limit_past_the_60_m_s_observation_clamp_cannot_free_the_ai() {
    // At 70 m/s the observation's body velocity sits on its 60 m/s clamp, so the policy
    // would see a wrong speed whatever limit was validated; only a new observation
    // range (a schema change) can lift that.
    let (ai_steps, outside_steps, _) = fly(world(70.0, [0.0; 3]), hybrid(Some(80.0)), 20);

    assert_eq!(ai_steps, 0);
    assert_eq!(outside_steps, 20);
}

#[test]
fn the_shipped_limit_is_55_m_s() {
    let controllers = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();

    assert_eq!(controllers.hybrid.policy_max_speed_m_s, None);
    assert_eq!(gate(None).envelope.max_speed_m_s, 55.0);
}
