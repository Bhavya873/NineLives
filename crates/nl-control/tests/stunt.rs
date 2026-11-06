//! The stunt planner (plan 2026-10-27 G4): the four authorization rules, the integrated
//! rotation, and the per-tick replanning that aborts to Catch.

#![allow(clippy::unwrap_used)]

use std::f32::consts::TAU;

use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML, StuntConfig};
use nl_control::stunt::{
    NozzleOwner, PlannerInputs, StuntEvent, StuntKind, StuntMode, StuntPhase, StuntPlanner,
    StuntReason, authorize, landing_reserve_kg,
};
use nl_sim::car_state::CarState;
use nl_sim::forecast::LandingForecast;

const DT: f32 = 1.0 / 60.0;

fn config() -> StuntConfig {
    ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML)
        .unwrap()
        .stunt
}

fn good_forecast(time_s: f32) -> LandingForecast {
    LandingForecast {
        time_s,
        point_m: [50.0, 0.0, 0.0],
        normal: [0.0, 1.0, 0.0],
        normal_tilt_deg: 2.0,
        usable_length_m: 30.0,
        time_uncertainty_s: 0.05,
        position_uncertainty_m: 1.0,
    }
}

fn airborne_state() -> CarState {
    CarState {
        gravity_dir_body: [0.0, -1.0, 0.0],
        angular_velocity_body_rad_s: [0.0; 3],
        linear_velocity_body_m_s: [20.0, 0.0, 0.0],
        height_m: 30.0,
        wheel_contact: [false; 4],
        mode_thrust_scale: 1.0,
        tank_gate: 1.0,
    }
}

/// The car on a ballistic arc that comes down `air_time_s` from now: it still rises at
/// half of g times that, so it lands at the same speed (the reserve counts the fall speed).
fn airborne_state_for(air_time_s: f32) -> CarState {
    let mut state = airborne_state();
    state.linear_velocity_body_m_s[1] = 0.5 * 9.80665 * air_time_s;
    state
}

fn inputs(time_s: f32, air_time_s: f32) -> PlannerInputs {
    PlannerInputs {
        time_s,
        dt_s: DT,
        state: airborne_state_for(air_time_s),
        air_kg: 260.0,
        mode: StuntMode::Realistic,
        unlimited_air: false,
        forecast: Some(good_forecast(air_time_s)),
    }
}

fn refusal(kind: StuntKind, input: &PlannerInputs) -> StuntReason {
    authorize(&config(), kind, input).unwrap_err().reason
}

// ---- Authorization rules ----

#[test]
fn a_good_flip_is_authorized_with_the_research_budget() {
    let plan = authorize(&config(), StuntKind::FrontFlip, &inputs(0.0, 4.0)).unwrap();
    assert_eq!(plan.coast_rate_rad_s, 5.0);
    // research §3: spin-up 0.38 + coast 0.9 + brake 0.27 + catch 0.8 + margin 0.3.
    assert!((plan.required_s - 2.64).abs() < 0.05, "{}", plan.required_s);
    // The stunt's own air (about 46 kg) plus the landing reserve from the forecast: the
    // fall speed at touchdown (about 19.6 m/s here) times `landing_air_kg_per_m_s`.
    let input = inputs(0.0, 4.0);
    let reserve = landing_reserve_kg(&config(), &input.state, &input.forecast.unwrap());
    assert!((reserve - 6.0 * 19.6).abs() < 1.0, "{reserve}");
    assert!(
        (plan.air_needed_kg - (46.0 + reserve)).abs() < 3.0,
        "{}",
        plan.air_needed_kg
    );
}

#[test]
fn rule_1_refuses_a_tilted_landing_surface() {
    let mut input = inputs(0.0, 4.0);
    input.forecast.as_mut().unwrap().normal_tilt_deg = 16.0;
    assert_eq!(
        refusal(StuntKind::BackFlip, &input),
        StuntReason::LandingTilted
    );
    input.forecast.as_mut().unwrap().normal_tilt_deg = 14.0;
    assert!(authorize(&config(), StuntKind::BackFlip, &input).is_ok());
}

#[test]
fn rule_1_refuses_a_short_or_blocked_landing() {
    let mut input = inputs(0.0, 4.0);
    // two car lengths = 8.4 m
    input.forecast.as_mut().unwrap().usable_length_m = 8.3;
    assert_eq!(
        refusal(StuntKind::BackFlip, &input),
        StuntReason::LandingBlocked
    );
    input.forecast.as_mut().unwrap().usable_length_m = 0.0;
    assert_eq!(
        refusal(StuntKind::BackFlip, &input),
        StuntReason::LandingBlocked
    );
    input.forecast.as_mut().unwrap().usable_length_m = 8.5;
    assert!(authorize(&config(), StuntKind::BackFlip, &input).is_ok());
}

#[test]
fn rule_1_refuses_without_a_forecast() {
    let mut input = inputs(0.0, 4.0);
    input.forecast = None;
    assert_eq!(
        refusal(StuntKind::FrontFlip, &input),
        StuntReason::NoLanding
    );
}

#[test]
fn rule_2_counts_the_uncertainty_against_the_air_time() {
    // Needs 2.64 s: 2.70 s of air with 0.05 s of uncertainty passes, 0.10 s fails.
    let mut input = inputs(0.0, 2.70);
    assert!(authorize(&config(), StuntKind::FrontFlip, &input).is_ok());
    input.forecast.as_mut().unwrap().time_uncertainty_s = 0.10;
    assert_eq!(
        refusal(StuntKind::FrontFlip, &input),
        StuntReason::NotEnoughTime
    );
    // A short hop is refused outright.
    assert_eq!(
        refusal(StuntKind::FrontFlip, &inputs(0.0, 1.0)),
        StuntReason::NotEnoughTime
    );
}

#[test]
fn rule_3_refuses_when_the_tank_cannot_pay_for_the_stunt_and_the_landing() {
    let mut input = inputs(0.0, 4.0);
    input.air_kg = 150.0; // flip needs about 164 kg: 46 for the stunt, 118 for the landing
    assert_eq!(
        refusal(StuntKind::FrontFlip, &input),
        StuntReason::NotEnoughAir
    );
    input.air_kg = 180.0;
    assert!(authorize(&config(), StuntKind::FrontFlip, &input).is_ok());
    // A harder landing (more fall speed at touchdown) needs more air for the same stunt.
    let mut hard = input;
    hard.state.linear_velocity_body_m_s[1] = 30.0;
    assert_eq!(
        refusal(StuntKind::FrontFlip, &hard),
        StuntReason::NotEnoughAir
    );
    // Arcade air never runs out: only the time rules apply.
    input.air_kg = 1.0;
    input.unlimited_air = true;
    input.mode = StuntMode::Arcade;
    input.state.mode_thrust_scale = 2.0;
    assert!(authorize(&config(), StuntKind::FrontFlip, &input).is_ok());
}

#[test]
fn rule_4_refuses_outside_the_start_envelope() {
    let kind = StuntKind::BarrelLeft;
    let mut grounded = inputs(0.0, 4.0);
    grounded.state.wheel_contact = [true, true, false, false];
    assert_eq!(refusal(kind, &grounded), StuntReason::NotAirborne);

    let mut tilted = inputs(0.0, 4.0);
    let tilt = 30.0_f32.to_radians();
    tilted.state.gravity_dir_body = [tilt.sin(), -tilt.cos(), 0.0];
    assert_eq!(refusal(kind, &tilted), StuntReason::TiltedTooFar);

    let mut spinning = inputs(0.0, 4.0);
    spinning.state.angular_velocity_body_rad_s = [0.0, 0.0, 2.0];
    assert_eq!(refusal(kind, &spinning), StuntReason::SpinningTooFast);

    let mut low_tank = inputs(0.0, 4.0);
    low_tank.state.tank_gate = 0.8;
    assert_eq!(refusal(kind, &low_tank), StuntReason::TankNotFull);

    let mut no_mode = inputs(0.0, 4.0);
    no_mode.state.mode_thrust_scale = 0.0;
    assert_eq!(refusal(kind, &no_mode), StuntReason::ModeUnknown);

    assert!(authorize(&config(), kind, &inputs(0.0, 4.0)).is_ok());
}

// ---- The state machine ----

#[test]
fn a_refusal_emits_stunt_refused_and_returns_to_idle() {
    let mut planner = StuntPlanner::new(config());
    assert!(planner.request(StuntKind::FrontFlip, 0.0));
    let tick = planner.tick(&inputs(0.0, 1.0));
    assert_eq!(planner.phase(), StuntPhase::Idle);
    assert_eq!(
        tick.events,
        vec![StuntEvent::Refused {
            kind: StuntKind::FrontFlip,
            reason: StuntReason::NotEnoughTime
        }]
    );
    assert_eq!(tick.owner, NozzleOwner::Pd);
}

#[test]
fn a_request_waits_for_take_off_and_expires_after_3_s() {
    let mut planner = StuntPlanner::new(config());
    let mut grounded = inputs(0.0, 4.0);
    grounded.state.wheel_contact = [true; 4];
    planner.request(StuntKind::BackFlip, 0.0);
    for step in 0..170 {
        grounded.time_s = step as f32 * DT; // up to 2.8 s
        let tick = planner.tick(&grounded);
        assert_eq!(tick.phase, StuntPhase::Requested);
        assert!(tick.events.is_empty());
    }
    grounded.time_s = 3.05;
    let tick = planner.tick(&grounded);
    assert_eq!(tick.phase, StuntPhase::Idle);
    assert_eq!(
        tick.events,
        vec![StuntEvent::Refused {
            kind: StuntKind::BackFlip,
            reason: StuntReason::RequestExpired
        }]
    );
    // A request that waited does start once airborne in time.
    planner.request(StuntKind::BackFlip, 10.0);
    grounded.time_s = 10.5;
    planner.tick(&grounded);
    let tick = planner.tick(&inputs(11.0, 4.0));
    assert_eq!(tick.phase, StuntPhase::SpinUp);
}

#[test]
fn a_second_request_is_ignored_while_one_is_active() {
    let mut planner = StuntPlanner::new(config());
    assert!(planner.request(StuntKind::BackFlip, 0.0));
    assert!(!planner.request(StuntKind::FrontFlip, 0.1));
    assert_eq!(planner.kind(), StuntKind::BackFlip);
}

#[test]
fn the_signs_follow_the_body_axes() {
    // Rotation about +Z takes the nose (+X) up: back flip +, front flip -.
    let nose_after = |angle: f32| [angle.cos(), angle.sin()]; // rotate (1,0) about +Z
    assert!(nose_after(0.1 * StuntKind::BackFlip.sign())[1] > 0.0);
    assert!(nose_after(0.1 * StuntKind::FrontFlip.sign())[1] < 0.0);
    // Rotation about +X takes up (+Y) towards right (+Z): the roof tips right, so the
    // right side goes down.
    let up_after = |angle: f32| [angle.cos(), angle.sin()]; // rotate (y, z)
    assert!(up_after(0.1 * StuntKind::BarrelRight.sign())[1] > 0.0);
    assert!(up_after(0.1 * StuntKind::BarrelLeft.sign())[1] < 0.0);
    assert_eq!(StuntKind::FrontFlip.axis(), 2);
    assert_eq!(StuntKind::BarrelRight.axis(), 0);
}

/// A kinematic stand-in for the car: `omega` is the progress rate about the stunt axis.
struct Rig {
    planner: StuntPlanner,
    kind: StuntKind,
    time_s: f32,
    air_left_s: f32,
    omega: f32,
    alpha: f32,
    uncertainty_s: f32,
    on_ground: bool,
    events: Vec<StuntEvent>,
    phases: Vec<StuntPhase>,
}

impl Rig {
    fn new(kind: StuntKind, air_s: f32) -> Rig {
        let mut planner = StuntPlanner::new(config());
        planner.request(kind, 0.0);
        Rig {
            planner,
            kind,
            time_s: 0.0,
            air_left_s: air_s,
            omega: 0.0,
            alpha: config().pitch_alpha_rad_s2,
            uncertainty_s: 0.05,
            on_ground: false,
            events: Vec::new(),
            phases: Vec::new(),
        }
    }

    fn step(&mut self) {
        let mut input = inputs(self.time_s, self.air_left_s);
        input.state.angular_velocity_body_rad_s[self.kind.axis()] = self.kind.sign() * self.omega;
        if self.on_ground {
            input.state.wheel_contact = [true; 4];
            input.forecast = None;
        } else {
            let forecast = input.forecast.as_mut().unwrap();
            forecast.time_uncertainty_s = self.uncertainty_s;
        }
        let tick = self.planner.tick(&input);
        self.events.extend(tick.events.iter().copied());
        if self.phases.last() != Some(&tick.phase) {
            self.phases.push(tick.phase);
        }
        // What the nozzles then do to the spin, then the car moves for one tick.
        match (tick.phase, tick.command) {
            (StuntPhase::SpinUp, Some(command)) => {
                assert_eq!(command.direction, self.kind.sign());
                self.omega += self.alpha * DT;
            }
            (StuntPhase::Brake, Some(command)) => {
                assert_eq!(command.direction, -self.kind.sign());
                self.omega = (self.omega - self.alpha * DT).max(0.0);
            }
            // PD (Catch, Aborted) damps the spin at its own comfortable rate.
            (StuntPhase::Catch | StuntPhase::Aborted, _) => {
                self.omega = (self.omega - 12.0 * DT).max(0.0);
            }
            _ => {}
        }
        self.time_s += DT;
        self.air_left_s -= DT;
        if self.air_left_s <= 0.0 {
            self.on_ground = true;
        }
    }
}

#[test]
fn a_full_flip_runs_every_phase_and_lands_with_the_full_rotation() {
    let mut rig = Rig::new(StuntKind::FrontFlip, 3.6);
    for _ in 0..(8 * 60) {
        rig.step();
    }
    assert_eq!(
        rig.phases[..6],
        [
            StuntPhase::SpinUp,
            StuntPhase::Coast,
            StuntPhase::Brake,
            StuntPhase::Catch,
            StuntPhase::Settled,
            StuntPhase::Idle
        ]
    );
    let landed: Vec<_> = rig
        .events
        .iter()
        .filter(|e| matches!(e, StuntEvent::Landed { .. }))
        .collect();
    assert_eq!(
        rig.events[0],
        StuntEvent::Started {
            kind: StuntKind::FrontFlip
        }
    );
    assert_eq!(landed.len(), 1, "{:?}", rig.events);
    let StuntEvent::Landed { rotation_rad, .. } = landed[0] else {
        unreachable!()
    };
    assert!(*rotation_rad >= TAU - 0.15, "{rotation_rad}");
    assert!(*rotation_rad < TAU + 0.6, "{rotation_rad}");
    assert!(
        !rig.events
            .iter()
            .any(|e| matches!(e, StuntEvent::Aborted { .. }))
    );
}

#[test]
fn nozzle_ownership_is_unique_each_tick_and_the_hybrid_gate_holds() {
    let mut rig = Rig::new(StuntKind::BackFlip, 3.6);
    let mut held_until_idle = true;
    for _ in 0..(8 * 60) {
        rig.step();
        let phase = rig.planner.phase();
        let owner = phase.owner();
        let director = matches!(
            phase,
            StuntPhase::SpinUp | StuntPhase::Coast | StuntPhase::Brake
        );
        assert_eq!(owner == NozzleOwner::StuntDirector, director);
        held_until_idle &= rig.planner.hybrid_hold_active() == (phase != StuntPhase::Idle);
    }
    assert!(held_until_idle);
    // Settled keeps the hold for 1 s, then Idle releases it.
    assert_eq!(rig.planner.phase(), StuntPhase::Idle);
    assert!(!rig.planner.hybrid_hold_active());
}

#[test]
fn a_barrel_roll_uses_the_roll_axis_and_its_faster_alpha() {
    let mut rig = Rig::new(StuntKind::BarrelRight, 3.0);
    rig.alpha = config().roll_alpha_rad_s2;
    for _ in 0..(8 * 60) {
        rig.step();
    }
    assert!(
        rig.events.iter().any(|e| matches!(
            e,
            StuntEvent::Landed {
                kind: StuntKind::BarrelRight,
                ..
            }
        )),
        "{:?}",
        rig.events
    );
}

#[test]
fn the_window_shrinking_aborts_to_catch_and_emits_stunt_aborted() {
    let mut rig = Rig::new(StuntKind::FrontFlip, 3.6);
    // Fly the spin-up and some coast, then the forecast says the landing is much closer
    // (a lower surface than expected becomes a higher one).
    for _ in 0..40 {
        rig.step();
    }
    assert!(matches!(
        rig.planner.phase(),
        StuntPhase::SpinUp | StuntPhase::Coast
    ));
    rig.air_left_s = 1.0; // less than catch + margin + the rest of the stunt
    rig.step();
    assert_eq!(rig.planner.phase(), StuntPhase::Aborted);
    assert_eq!(rig.planner.phase().owner(), NozzleOwner::Pd);
    let aborted: Vec<_> = rig
        .events
        .iter()
        .filter(|e| matches!(e, StuntEvent::Aborted { .. }))
        .collect();
    assert_eq!(aborted.len(), 1);
    assert!(matches!(
        aborted[0],
        StuntEvent::Aborted {
            reason: StuntReason::NotEnoughTime,
            ..
        }
    ));
    // PD recovers, the car lands, and no StuntLanded is claimed for a half-done flip.
    for _ in 0..(5 * 60) {
        rig.step();
    }
    assert!(rig.planner.rotation_rad() < TAU - 0.15);
    assert!(
        !rig.events
            .iter()
            .any(|e| matches!(e, StuntEvent::Landed { .. }))
    );
    assert_eq!(rig.events.len(), 2, "{:?}", rig.events); // Started, Aborted
}

#[test]
fn a_landing_that_becomes_blocked_or_tilted_aborts() {
    for (tilt, length, reason) in [
        (20.0, 30.0, StuntReason::LandingTilted),
        (2.0, 3.0, StuntReason::LandingBlocked),
    ] {
        let mut planner = StuntPlanner::new(config());
        planner.request(StuntKind::BackFlip, 0.0);
        planner.tick(&inputs(0.0, 4.0));
        let mut input = inputs(DT, 3.9);
        let landing = input.forecast.as_mut().unwrap();
        landing.normal_tilt_deg = tilt;
        landing.usable_length_m = length;
        let tick = planner.tick(&input);
        assert_eq!(tick.phase, StuntPhase::Aborted);
        assert!(matches!(
            tick.events[0],
            StuntEvent::Aborted { reason: r, .. } if r == reason
        ));
    }
}

#[test]
fn losing_the_forecast_or_touching_down_early_aborts() {
    let mut planner = StuntPlanner::new(config());
    planner.request(StuntKind::BackFlip, 0.0);
    planner.tick(&inputs(0.0, 4.0));
    let mut input = inputs(DT, 3.9);
    input.forecast = None;
    assert_eq!(planner.tick(&input).phase, StuntPhase::Aborted);

    let mut planner = StuntPlanner::new(config());
    planner.request(StuntKind::BackFlip, 0.0);
    planner.tick(&inputs(0.0, 4.0));
    let mut input = inputs(DT, 3.9);
    input.state.wheel_contact = [true, false, false, false];
    let tick = planner.tick(&input);
    assert!(matches!(
        tick.events[0],
        StuntEvent::Aborted {
            reason: StuntReason::ControlLost,
            ..
        }
    ));
}

#[test]
fn upright_at_the_end_is_not_a_stunt_without_the_full_rotation() {
    // Authorized, then (a dead tank) the spin never builds; the car lands upright.
    // No StuntLanded: the rotation is far short of 2 pi - 0.15.
    let mut planner = StuntPlanner::new(config());
    planner.request(StuntKind::FrontFlip, 0.0);
    planner.tick(&inputs(0.0, 4.0));
    let mut input = inputs(DT, 3.9);
    input.state.angular_velocity_body_rad_s[2] = -1.0; // turned a little
    planner.tick(&input);
    let mut landed = inputs(1.0, 0.0);
    landed.state.wheel_contact = [true; 4];
    landed.forecast = None;
    // Force the recovery path: abort first (window gone), then land.
    let mut shrunk = inputs(0.5, 0.5);
    shrunk.state.angular_velocity_body_rad_s[2] = -1.0;
    planner.tick(&shrunk);
    assert_eq!(planner.phase(), StuntPhase::Aborted);
    let tick = planner.tick(&landed);
    assert_eq!(tick.phase, StuntPhase::Settled);
    assert!(!planner.rotation_complete());
    assert!(
        !tick
            .events
            .iter()
            .any(|e| matches!(e, StuntEvent::Landed { .. }))
    );
}

#[test]
fn completion_needs_the_slack_exact_threshold() {
    // Integrate a constant 5 rad/s about the roll axis: complete at 2 pi - 0.15, not before.
    let mut planner = StuntPlanner::new(config());
    planner.request(StuntKind::BarrelRight, 0.0);
    planner.tick(&inputs(0.0, 6.0));
    let mut crossed_at = None;
    for step in 0..400 {
        let mut input = inputs(step as f32 * DT, 6.0);
        input.state.angular_velocity_body_rad_s[0] = 5.0;
        planner.tick(&input);
        if crossed_at.is_none() && planner.rotation_complete() {
            crossed_at = Some(planner.rotation_rad());
        }
    }
    let at = crossed_at.unwrap();
    assert!(
        (TAU - 0.15..TAU - 0.15 + 5.0 * DT + 1e-3).contains(&at),
        "{at}"
    );
}

// ---- Against a real world ----

#[test]
fn a_world_forecast_feeds_the_planner() {
    use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
    use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
    use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};
    let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
    let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
        .unwrap()
        .realistic;
    let mut world = World::new_on_flat_ground(&vehicle, mode, DEFAULT_GROUND_FRICTION, 1);
    world
        .place_car(
            [0.0, 60.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [15.0, 0.0, 0.0],
            [0.0; 3],
        )
        .unwrap();
    // The wheels' rays have not been cast yet, so no wheel reads as touching.
    let mut planner = StuntPlanner::new(config());
    planner.request(StuntKind::FrontFlip, 0.0);
    let tick = planner.tick_world(&world, DT);
    assert_eq!(
        tick.events,
        vec![StuntEvent::Started {
            kind: StuntKind::FrontFlip
        }]
    );
    assert_eq!(tick.phase, StuntPhase::SpinUp);
    assert_eq!(tick.command.unwrap().direction, -1.0);

    // From 3 m up there is no time for it.
    world
        .place_car(
            [0.0, 3.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [15.0, 0.0, 0.0],
            [0.0; 3],
        )
        .unwrap();
    let mut planner = StuntPlanner::new(config());
    planner.request(StuntKind::FrontFlip, 0.0);
    let tick = planner.tick_world(&world, DT);
    assert_eq!(
        tick.events,
        vec![StuntEvent::Refused {
            kind: StuntKind::FrontFlip,
            reason: StuntReason::NotEnoughTime
        }]
    );
}

#[test]
fn a_completed_turn_stays_complete_when_pd_turns_the_car_back_to_level() {
    // A ramp launch leaves the car nose up: after a full turn PD levels it, which turns the
    // car back and shrinks the counted rotation. The turn already made still counts.
    let mut planner = StuntPlanner::new(config());
    planner.request(StuntKind::BackFlip, 0.0);
    planner.tick(&inputs(0.0, 4.0));
    let sign = StuntKind::BackFlip.sign();
    let mut time_s = DT;
    let mut spin = |planner: &mut StuntPlanner, rate: f32, count: usize| {
        for _ in 0..count {
            let mut input = inputs(time_s, 3.5);
            input.state.angular_velocity_body_rad_s[StuntKind::BackFlip.axis()] = sign * rate;
            planner.tick(&input);
            time_s += DT;
        }
    };
    spin(&mut planner, 6.4, 66); // about 7 rad: a full turn
    assert!(planner.rotation_complete());
    spin(&mut planner, -3.0, 60); // PD levelling: counted rotation drops by about 3 rad
    assert!(planner.rotation_rad() < std::f32::consts::TAU - 0.15);
    assert!(planner.rotation_complete(), "the latch must hold");
    let mut landed = inputs(time_s, 0.0);
    landed.state.wheel_contact = [true; 4];
    landed.forecast = None;
    let tick = planner.tick(&landed);
    assert!(
        tick.events
            .iter()
            .any(|e| matches!(e, StuntEvent::Landed { .. })),
        "{:?}",
        tick.events
    );
}

#[test]
fn a_queued_request_waits_while_a_ramp_launch_has_the_car_tilted_too_far() {
    let mut planner = StuntPlanner::new(config());
    planner.request(StuntKind::BackFlip, 0.0);
    let mut tilted = inputs(0.0, 4.0);
    tilted.state.gravity_dir_body = [0.8, -0.6, 0.0]; // about 53 degrees from upright
    let tick = planner.tick(&tilted);
    assert_eq!(planner.phase(), StuntPhase::Requested, "{:?}", tick.events);
    assert!(tick.events.is_empty());
    // Levelled a moment later, the same request is authorized.
    let tick = planner.tick(&inputs(0.1, 3.9));
    assert!(matches!(tick.events[0], StuntEvent::Started { .. }));
}
