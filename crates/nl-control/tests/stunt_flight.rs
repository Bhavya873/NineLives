//! The stunt director flying a real car (plan G4): a requested stunt spins the car through
//! the full rotation, PD catches it, and it lands on its wheels without crashing.

#![allow(clippy::unwrap_used)]

use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};
use nl_control::stunt::{StuntDirector, StuntEvent, StuntKind, StuntPhase};
use nl_control::{ControlCtx, Controller, Pd};
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// What one flown stunt did.
struct Flight {
    events: Vec<StuntEvent>,
    crashed: bool,
    wheels_down_at_end: usize,
    final_phase: StuntPhase,
}

/// Drops a car from `height_m` at `speed_m_s`, requests `kind` on the first tick and flies
/// the stunt director with PD as the fallback for `seconds`.
fn fly(kind: StuntKind, height_m: f32, speed_m_s: f32, seconds: f32) -> Flight {
    let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
    let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
        .unwrap()
        .realistic;
    let controllers = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();
    let mut world = World::new_on_flat_ground(&vehicle, mode, DEFAULT_GROUND_FRICTION, 1);
    world
        .place_car(
            [0.0, height_m, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [speed_m_s, 0.0, 0.0],
            [0.0; 3],
        )
        .unwrap();
    let pd = Pd::for_vehicle(&vehicle, mode, &controllers);
    let mut fallback = pd.clone();
    let mut director = StuntDirector::new(controllers.stunt, pd);
    assert!(director.request(kind, 0.0));
    let mut events = Vec::new();
    let steps = (seconds * 120.0) as usize;
    for _ in 0..steps {
        if world.is_control_step() {
            let ctx = ControlCtx::for_world(&world, InputFrame::default());
            let step = director.step(&world, &ctx, 1.0 / 60.0);
            events.extend(step.tick.events.iter().copied());
            let throttles = step.throttles.unwrap_or_else(|| fallback.act(&ctx));
            world.set_controller_throttles(throttles);
        }
        world.step(InputFrame::default());
    }
    Flight {
        events,
        crashed: world.is_crashed(),
        wheels_down_at_end: world.wheels_touching_count(),
        final_phase: director.planner().phase(),
    }
}

#[test]
fn every_stunt_kind_completes_a_full_rotation_and_lands() {
    for kind in [
        StuntKind::FrontFlip,
        StuntKind::BackFlip,
        StuntKind::BarrelLeft,
        StuntKind::BarrelRight,
    ] {
        let flight = fly(kind, 60.0, 15.0, 8.0);
        println!(
            "{kind:?}: {:?} phase {:?}",
            flight.events, flight.final_phase
        );
        assert!(!flight.crashed, "{kind:?} crashed: {:?}", flight.events);
        let landed = flight
            .events
            .iter()
            .any(|event| matches!(event, StuntEvent::Landed { rotation_rad, .. } if *rotation_rad >= std::f32::consts::TAU - 0.15));
        assert!(landed, "{kind:?} did not land: {:?}", flight.events);
        assert_eq!(flight.wheels_down_at_end, 4);
    }
}

/// A fast fall (over 40 m/s) can step from just above the ground to just inside it; the
/// forecast must still find a level surface and not refuse every stunt as tilted.
#[test]
fn a_forecast_from_a_very_high_drop_finds_level_ground() {
    let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
    let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
        .unwrap()
        .realistic;
    for height_m in [100.0_f32, 130.0, 150.0] {
        let mut world = World::new_on_flat_ground(&vehicle, mode, DEFAULT_GROUND_FRICTION, 1);
        world
            .place_car(
                [0.0, height_m, 0.0],
                [0.0, 0.0, 0.0, 1.0],
                [5.0, 0.0, 0.0],
                [0.0; 3],
            )
            .unwrap();
        let forecast = world.forecast_landing().expect("the car lands");
        assert!(forecast.normal_tilt_deg < 1.0, "{height_m} m: {forecast:?}");
    }
}
