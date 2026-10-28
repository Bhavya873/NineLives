//! Stunt budget (research R1, docs/research/stunt-planning.md): measures, through the real
//! allocator and the real sim, what a planned flip or barrel roll costs. Per mode and axis:
//! the angular acceleration the nozzles reach, then for a range of coast rates the
//! spin-up → coast → (brake →) PD catch manoeuvre's time and air; and the airtime off
//! every playground ramp. Prints tables and writes `results/stunt_budget.json`.
//!
//! cargo run --release -p nl-control --example stunt_budget   (from the repo root)

use std::error::Error;
use std::f32::consts::{PI, TAU};

use nl_control::allocation::{ActuatorPlan, AllocationConfig, Allocator};
use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};
use nl_control::{ControlCtx, Controller, Pd};
use nl_sim::car_state::{CarModel, WRENCH_SIZE, Wrench};
use nl_sim::conventions::{GRAVITY_M_S2, NOZZLE_COUNT, PHYSICS_DT_S};
use nl_sim::course::{CourseSpec, PieceSpec};
use nl_sim::events::SimEvent;
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModeParams, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};
use serde_json::{Value, json};

/// Spin tests run this high, far from the ground, so only the nozzles turn the car.
const SPIN_TEST_ALTITUDE_M: f32 = 300.0;
/// The angular acceleration asked of the allocator: far beyond reach, so it saturates.
const UNREACHABLE_ALPHA_RAD_S2: f32 = 500.0;
/// How long full stunt torque is held to measure the angular acceleration.
const ALPHA_TEST_S: f32 = 0.5;
/// The acceleration is the slope of the spin rate between these times, in s (after the
/// valves have opened, SPEC §7.4 valve lag).
const ALPHA_FIT_START_S: f32 = 0.15;
const ALPHA_FIT_END_S: f32 = 0.45;
/// Coast rates tried for a full 2π turn, in rad/s.
const COAST_RATES_RAD_S: [f32; 6] = [3.0, 4.0, 5.0, 6.0, 8.0, 10.0];
/// The stunt's own brake hands over to PD at this spin rate, in rad/s.
const HANDOVER_RATE_RAD_S: f32 = 1.5;
/// The catch is over when the car is within this tilt of upright...
const SETTLED_TILT_DEG: f32 = 5.0;
/// ...and turning slower than this, in rad/s.
const SETTLED_SPIN_RAD_S: f32 = 0.5;
/// A phase that has not finished by then gives up, in s.
const PHASE_TIMEOUT_S: f32 = 4.0;
/// Playground ramp approach: the car starts this far before the ramp foot, in m...
const APPROACH_M: f32 = 25.0;
/// ...with its centre of mass this high (the course start height), in m.
const START_HEIGHT_M: f32 = 0.7;
/// Ramp speeds tried, in m/s.
const RAMP_SPEEDS_M_S: [f32; 4] = [15.0, 20.0, 25.0, 30.0];
/// Throttle per m/s of speed error for the approach's cruise control.
const CRUISE_GAIN_PER_M_S: f32 = 0.5;
/// A ramp run gives up after this long, in s.
const RAMP_RUN_TIMEOUT_S: f32 = 10.0;
/// "Landing air" counts up to this long after touchdown, in s.
const AFTER_LANDING_S: f32 = 0.5;

/// A stunt's rotation: about which body axis, and which way.
#[derive(Clone, Copy)]
struct StuntAxis {
    name: &'static str,
    /// 0 = body X (roll), 2 = body Z (pitch), SPEC §4 axes.
    body_axis: usize,
    /// +1 or −1: the sign of the body angular velocity the stunt needs.
    sign: f32,
}

/// SPEC §4 signs: +ω_z turns +X toward +Y (nose up), so a front flip (nose down) is −ω_z;
/// +ω_x turns +Y toward +Z and +Z toward −Y, so the right side goes down: barrel right.
const STUNT_AXES: [StuntAxis; 4] = [
    StuntAxis {
        name: "front_flip",
        body_axis: 2,
        sign: -1.0,
    },
    StuntAxis {
        name: "back_flip",
        body_axis: 2,
        sign: 1.0,
    },
    StuntAxis {
        name: "barrel_right",
        body_axis: 0,
        sign: 1.0,
    },
    StuntAxis {
        name: "barrel_left",
        body_axis: 0,
        sign: -1.0,
    },
];

/// Everything the measurements read, loaded once.
struct Setup {
    vehicle: VehicleConfig,
    controllers: ControllersConfig,
    modes: [(&'static str, ModeParams); 2],
    course: CourseSpec,
}

/// What full stunt torque does about one axis.
struct AlphaResult {
    alpha_rad_s2: f32,
    air_kg_s: f32,
}

/// How the spin is stopped at the end of the turn.
#[derive(Clone, Copy, PartialEq)]
enum CatchStyle {
    /// PD takes over while the car still spins at the coast rate (plan §1.5 as written).
    PdOnly,
    /// The stunt brakes with full counter-torque down to `HANDOVER_RATE_RAD_S`, then PD.
    BrakeThenPd,
}

/// One spin-up → coast → (brake →) catch manoeuvre. Times in s, air in kg.
struct FlipResult {
    style: CatchStyle,
    coast_rate_rad_s: f32,
    spin_up_s: f32,
    coast_s: f32,
    brake_s: f32,
    /// From the PD hand-over until the car could land: PD's quiet tilt and spin limits.
    catch_to_landable_s: f32,
    /// From the PD hand-over until tilt < `SETTLED_TILT_DEG` and spin < `SETTLED_SPIN_RAD_S`.
    catch_to_settled_s: f32,
    spin_up_air_kg: f32,
    brake_air_kg: f32,
    catch_to_landable_air_kg: f32,
    rotation_rad: f32,
    settled: bool,
}

impl FlipResult {
    /// Manoeuvre time until the car could land.
    fn time_to_landable_s(&self) -> f32 {
        self.spin_up_s + self.coast_s + self.brake_s + self.catch_to_landable_s
    }

    /// Air used until the car could land.
    fn air_to_landable_kg(&self) -> f32 {
        self.spin_up_air_kg + self.brake_air_kg + self.catch_to_landable_air_kg
    }
}

/// One run off a playground ramp.
struct RampResult {
    piece: usize,
    length_m: f32,
    angle_deg: f32,
    speed_m_s: f32,
    /// The run with the nozzles off: ballistic airtime.
    off: JumpLog,
    /// The run with PD flying: the air a landing costs.
    pd: JumpLog,
}

fn main() -> Result<(), Box<dyn Error>> {
    let setup = load_setup()?;
    let mut report_modes = Vec::new();
    for (mode_name, mode) in setup.modes {
        report_modes.push(measure_mode(&setup, mode_name, mode)?);
    }
    let ramps = measure_ramps(&setup)?;
    print_ramp_table(&ramps);
    let report = json!({
        "note": "docs/research/stunt-planning.md; cargo run --release -p nl-control --example stunt_budget",
        "modes": report_modes,
        "ramps": ramps.iter().map(ramp_json).collect::<Vec<Value>>(),
    });
    std::fs::create_dir_all("results")?;
    std::fs::write(
        "results/stunt_budget.json",
        serde_json::to_string_pretty(&report)?,
    )?;
    println!("\nwrote results/stunt_budget.json");
    Ok(())
}

fn load_setup() -> Result<Setup, Box<dyn Error>> {
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)?;
    let realistic = modes
        .by_name("realistic")
        .ok_or("modes.toml has no [realistic]")?;
    let arcade = modes
        .by_name("arcade")
        .ok_or("modes.toml has no [arcade]")?;
    let course_text = std::fs::read_to_string("courses/playground.json")
        .map_err(|error| format!("courses/playground.json: {error} (run from the repo root)"))?;
    Ok(Setup {
        vehicle: VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML)?,
        controllers: ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML)?,
        modes: [("realistic", realistic), ("arcade", arcade)],
        course: CourseSpec::from_json_str(&course_text)?,
    })
}

// ---------------------------------------------------------------------------------------
// Angular acceleration and the flip sweep
// ---------------------------------------------------------------------------------------

/// α and the flip sweep for every stunt axis in one mode; prints its tables.
fn measure_mode(setup: &Setup, mode_name: &str, mode: ModeParams) -> Result<Value, Box<dyn Error>> {
    let model = spin_test_world(setup, mode, 0.0)?.car_model();
    println!(
        "\n=== {mode_name}: mass {:.0} kg, inertia {:?} kg·m² ===",
        model.mass_kg, model.principal_inertia_kg_m2
    );
    let torque_only = AllocationConfig {
        force_weight: 0.0,
        ..setup.controllers.allocation
    };
    let mut axes = Vec::new();
    for axis in STUNT_AXES {
        let pd_weights = measure_alpha(setup, mode, axis, &setup.controllers.allocation)?;
        let torque_weights = measure_alpha(setup, mode, axis, &torque_only)?;
        println!(
            "\n{} α {:.1} rad/s² ({:.1} kg/s air); torque-only weights α {:.1} rad/s² ({:.1} kg/s)",
            axis.name,
            pd_weights.alpha_rad_s2,
            pd_weights.air_kg_s,
            torque_weights.alpha_rad_s2,
            torque_weights.air_kg_s
        );
        let flips = sweep_coast_rates(setup, mode, axis, pd_weights.alpha_rad_s2)?;
        axes.push(json!({
            "axis": axis.name,
            "alpha_rad_s2": pd_weights.alpha_rad_s2,
            "air_kg_s": pd_weights.air_kg_s,
            "alpha_torque_only_rad_s2": torque_weights.alpha_rad_s2,
            "air_torque_only_kg_s": torque_weights.air_kg_s,
            "bang_bang_2pi_s": (8.0 * PI / pd_weights.alpha_rad_s2).sqrt(),
            "flips": flips.iter().map(flip_json).collect::<Vec<Value>>(),
        }));
    }
    Ok(json!({
        "mode": mode_name,
        "mass_kg": model.mass_kg,
        "inertia_kg_m2": model.principal_inertia_kg_m2,
        "axes": axes,
    }))
}

/// A world with the car hanging level at `SPIN_TEST_ALTITUDE_M`, rising at `rising_m_s`.
fn spin_test_world(
    setup: &Setup,
    mode: ModeParams,
    rising_m_s: f32,
) -> Result<World, Box<dyn Error>> {
    let mut world = World::new_on_flat_ground(&setup.vehicle, mode, DEFAULT_GROUND_FRICTION, 0);
    let level = [0.0, 0.0, 0.0, 1.0];
    let position = [0.0, SPIN_TEST_ALTITUDE_M, 0.0];
    world.place_car(position, level, [0.0, rising_m_s, 0.0], [0.0; 3])?;
    Ok(world)
}

/// The wrench a stunt asks for: an unreachable torque about the stunt axis, no force.
fn stunt_wrench(inertia_kg_m2: [f32; 3], axis: StuntAxis) -> Wrench {
    let mut wrench = [0.0; WRENCH_SIZE];
    let torque_row = 3 + axis.body_axis;
    wrench[torque_row] = axis.sign * inertia_kg_m2[axis.body_axis] * UNREACHABLE_ALPHA_RAD_S2;
    wrench
}

/// Throttles for `wrench` from the actuator-aware allocation, exactly as PD plans them.
fn allocate(
    allocator: &Allocator,
    world: &World,
    wrench: Wrench,
    throttles: &mut [f32; NOZZLE_COUNT],
) {
    let actuators = world.actuator_state();
    if actuators.thrusters_locked() {
        *throttles = [0.0; NOZZLE_COUNT];
        return;
    }
    let idle = actuators.predict([0.0; NOZZLE_COUNT]);
    let plan = ActuatorPlan::from_idle_prediction(&idle, actuators.controller_gain());
    allocator.solve_actuator_aware(wrench, actuators.mode_thrust_scale(), &plan, throttles);
}

/// Fires `wrench` (re-allocated each control step) for one physics step.
fn step_with_wrench(
    world: &mut World,
    flight: &mut FlightLog,
    allocator: &Allocator,
    wrench: Wrench,
    throttles: &mut [f32; NOZZLE_COUNT],
    axis: StuntAxis,
) {
    if world.is_control_step() {
        allocate(allocator, world, wrench, throttles);
        world.set_controller_throttles(*throttles);
    }
    flight.step(world, axis);
}

/// The car's spin rate about the stunt axis, positive the stunt's way, in rad/s.
fn stunt_rate_rad_s(world: &World, axis: StuntAxis) -> f32 {
    axis.sign * world.angular_velocity_body_rad_s()[axis.body_axis]
}

/// Holds full stunt torque for `ALPHA_TEST_S` and fits the spin rate's slope.
fn measure_alpha(
    setup: &Setup,
    mode: ModeParams,
    axis: StuntAxis,
    config: &AllocationConfig,
) -> Result<AlphaResult, Box<dyn Error>> {
    let mut world = spin_test_world(setup, mode, 0.0)?;
    let model = world.car_model();
    let allocator = Allocator::new(&model.nozzle_wrenches, config);
    let wrench = stunt_wrench(model.principal_inertia_kg_m2, axis);
    let mut throttles = [0.0; NOZZLE_COUNT];
    let mut flight = FlightLog::default();
    let mut fit_start: Option<(f32, f32, f32)> = None;
    let mut fit_end = (0.0, 0.0, 0.0);
    while flight.time_s < ALPHA_TEST_S {
        step_with_wrench(
            &mut world,
            &mut flight,
            &allocator,
            wrench,
            &mut throttles,
            axis,
        );
        let sample = (
            flight.time_s,
            stunt_rate_rad_s(&world, axis),
            world.air_used_kg(),
        );
        if flight.time_s >= ALPHA_FIT_START_S && fit_start.is_none() {
            fit_start = Some(sample);
        }
        if flight.time_s <= ALPHA_FIT_END_S {
            fit_end = sample;
        }
    }
    let start = fit_start.unwrap_or(fit_end);
    let elapsed_s = (fit_end.0 - start.0).max(PHYSICS_DT_S);
    Ok(AlphaResult {
        alpha_rad_s2: (fit_end.1 - start.1) / elapsed_s,
        air_kg_s: (fit_end.2 - start.2) / elapsed_s,
    })
}

/// Runs the manoeuvre for every coast rate and both catch styles; prints one row each.
fn sweep_coast_rates(
    setup: &Setup,
    mode: ModeParams,
    axis: StuntAxis,
    alpha_rad_s2: f32,
) -> Result<Vec<FlipResult>, Box<dyn Error>> {
    println!(
        "  catch       ω | spin s coast s brake s catch→land s | to land s  air kg | spin kg brake kg catch kg | →settled s turned"
    );
    let mut results = Vec::new();
    for style in [CatchStyle::PdOnly, CatchStyle::BrakeThenPd] {
        for coast_rate in COAST_RATES_RAD_S {
            match run_flip(setup, mode, axis, alpha_rad_s2, coast_rate, style)? {
                Some(flip) => {
                    print_flip_row(&flip);
                    results.push(flip);
                }
                None => println!(
                    "  {:<8} {coast_rate:>4.1} | cannot reach and stop this rate within one turn",
                    style_name(style)
                ),
            }
        }
    }
    Ok(results)
}

fn style_name(style: CatchStyle) -> &'static str {
    match style {
        CatchStyle::PdOnly => "pd_only",
        CatchStyle::BrakeThenPd => "brake_pd",
    }
}

fn print_flip_row(flip: &FlipResult) {
    let settled_note = if flip.settled { "" } else { " NOT SETTLED" };
    println!(
        "  {:<8} {:>4.1} | {:>6.2} {:>7.2} {:>7.2} {:>12.2} | {:>9.2} {:>7.1} | {:>7.1} {:>8.1} {:>8.1} | {:>10.2} {:>6.2}{}",
        style_name(flip.style),
        flip.coast_rate_rad_s,
        flip.spin_up_s,
        flip.coast_s,
        flip.brake_s,
        flip.catch_to_landable_s,
        flip.time_to_landable_s(),
        flip.air_to_landable_kg(),
        flip.spin_up_air_kg,
        flip.brake_air_kg,
        flip.catch_to_landable_air_kg,
        flip.catch_to_settled_s,
        flip.rotation_rad,
        settled_note
    );
}

/// The turn left when the spin must start slowing, in rad. PD only: ω² / (2·α) with α the
/// smaller of the reachable acceleration and PD's comfort cap. Brake: full α from ω down
/// to the hand-over rate. Both add ω × the valve time constant, because the valves open
/// with a lag (SPEC §7.4).
fn stop_lead_rad(setup: &Setup, style: CatchStyle, rate_rad_s: f32, alpha_rad_s2: f32) -> f32 {
    let lag_rad = rate_rad_s * setup.vehicle.thrusters.valve_time_constant_s;
    match style {
        CatchStyle::PdOnly => {
            let alpha = alpha_rad_s2.min(setup.controllers.pd.max_angular_accel_rad_s2);
            rate_rad_s * rate_rad_s / (2.0 * alpha) + lag_rad
        }
        CatchStyle::BrakeThenPd => {
            let handover = HANDOVER_RATE_RAD_S;
            (rate_rad_s * rate_rad_s - handover * handover) / (2.0 * alpha_rad_s2) + lag_rad
        }
    }
}

/// Spin-up to `coast_rate`, coast to the stop point, (brake,) then PD until settled.
/// The car is thrown up so it reaches the top of its arc as the stopping starts, which
/// keeps PD's fall braking out of the catch (that air belongs to the landing). None when
/// the rate cannot be reached and stopped within one turn.
fn run_flip(
    setup: &Setup,
    mode: ModeParams,
    axis: StuntAxis,
    alpha_rad_s2: f32,
    coast_rate: f32,
    style: CatchStyle,
) -> Result<Option<FlipResult>, Box<dyn Error>> {
    let lead_rad = stop_lead_rad(setup, style, coast_rate, alpha_rad_s2);
    let spin_up_rad = coast_rate * coast_rate / (2.0 * alpha_rad_s2);
    let coast_rad = TAU - spin_up_rad - lead_rad;
    if alpha_rad_s2 <= 0.0 || coast_rad < 0.0 {
        return Ok(None);
    }
    let predicted_stop_start_s = coast_rate / alpha_rad_s2 + coast_rad / coast_rate;
    let mut world = spin_test_world(setup, mode, GRAVITY_M_S2 * predicted_stop_start_s)?;
    let model = world.car_model();
    let allocator = Allocator::new(&model.nozzle_wrenches, &setup.controllers.allocation);
    let spin_wrench = stunt_wrench(model.principal_inertia_kg_m2, axis);
    let mut throttles = [0.0; NOZZLE_COUNT];
    let mut flight = FlightLog::default();

    // SpinUp: full stunt torque until the coast rate is reached.
    while stunt_rate_rad_s(&world, axis) < coast_rate && flight.time_s < PHASE_TIMEOUT_S {
        step_with_wrench(
            &mut world,
            &mut flight,
            &allocator,
            spin_wrench,
            &mut throttles,
            axis,
        );
    }
    let spin_up_end = flight.mark(&world);

    // Coast: nozzles shut (the valves close with their lag) until the stop point.
    world.set_controller_throttles([0.0; NOZZLE_COUNT]);
    while flight.rotation_rad < TAU - lead_rad && flight.time_s < 2.0 * PHASE_TIMEOUT_S {
        flight.step(&mut world, axis);
    }
    let coast_end = flight.mark(&world);

    // Brake (BrakeThenPd only): full counter-torque down to the hand-over rate.
    if style == CatchStyle::BrakeThenPd {
        let brake_wrench = spin_wrench.map(|value| -value);
        let brake_start_s = flight.time_s;
        while stunt_rate_rad_s(&world, axis) > HANDOVER_RATE_RAD_S
            && flight.time_s - brake_start_s < PHASE_TIMEOUT_S
        {
            step_with_wrench(
                &mut world,
                &mut flight,
                &allocator,
                brake_wrench,
                &mut throttles,
                axis,
            );
        }
    }
    let brake_end = flight.mark(&world);

    // Catch: PD alone owns the nozzles until the car is settled upright.
    let catch = catch_with_pd(setup, &mut world, &model, axis, &mut flight);
    Ok(Some(FlipResult {
        style,
        coast_rate_rad_s: coast_rate,
        spin_up_s: spin_up_end.0,
        coast_s: coast_end.0 - spin_up_end.0,
        brake_s: brake_end.0 - coast_end.0,
        catch_to_landable_s: catch.landable.0 - brake_end.0,
        catch_to_settled_s: catch.settled.0 - brake_end.0,
        spin_up_air_kg: spin_up_end.1,
        brake_air_kg: brake_end.1 - coast_end.1,
        catch_to_landable_air_kg: catch.landable.1 - brake_end.1,
        rotation_rad: flight.rotation_rad,
        settled: catch.is_settled,
    }))
}

/// When PD's catch reached "landable" and "settled", as (time s, air used kg) marks.
struct CatchMarks {
    landable: (f32, f32),
    settled: (f32, f32),
    is_settled: bool,
}

/// PD flies until the car is settled upright or `PHASE_TIMEOUT_S` passes.
fn catch_with_pd(
    setup: &Setup,
    world: &mut World,
    model: &CarModel,
    axis: StuntAxis,
    flight: &mut FlightLog,
) -> CatchMarks {
    let mut pd = Pd::new(model, &setup.controllers);
    let start_s = flight.time_s;
    let mut landable: Option<(f32, f32)> = None;
    while flight.time_s - start_s < PHASE_TIMEOUT_S {
        if world.is_control_step() {
            if landable.is_none() && is_landable(setup, world) {
                landable = Some(flight.mark(world));
            }
            if is_settled(world) {
                let settled = flight.mark(world);
                let landable = landable.unwrap_or(settled);
                return CatchMarks {
                    landable,
                    settled,
                    is_settled: true,
                };
            }
            let throttles = pd.act(&ControlCtx::for_world(world, InputFrame::default()));
            world.set_controller_throttles(throttles);
        }
        flight.step(world, axis);
    }
    let timeout = flight.mark(world);
    let landable = landable.unwrap_or(timeout);
    CatchMarks {
        landable,
        settled: timeout,
        is_settled: false,
    }
}

/// Time and turned angle since a manoeuvre started; the angle integrates the body spin
/// about the stunt axis (plan G4: full rotation tracked, never just "upright at the end").
#[derive(Default)]
struct FlightLog {
    time_s: f32,
    rotation_rad: f32,
}

impl FlightLog {
    /// One physics step with no driver input.
    fn step(&mut self, world: &mut World, axis: StuntAxis) {
        world.step(InputFrame::default());
        self.time_s += PHYSICS_DT_S;
        self.rotation_rad += stunt_rate_rad_s(world, axis) * PHYSICS_DT_S;
    }

    /// (time so far in s, air used so far in kg).
    fn mark(&self, world: &World) -> (f32, f32) {
        (self.time_s, world.air_used_kg())
    }
}

/// Upright and calm enough to touch down: PD's own quiet limits (controllers.toml
/// `quiet_max_tilt_deg`, `quiet_max_spin_rad_s`).
fn is_landable(setup: &Setup, world: &World) -> bool {
    let pd = &setup.controllers.pd;
    world.tilt_rad() < pd.quiet_max_tilt_deg.to_radians()
        && spin_rad_s(world) < pd.quiet_max_spin_rad_s
}

fn is_settled(world: &World) -> bool {
    world.tilt_rad() < SETTLED_TILT_DEG.to_radians() && spin_rad_s(world) < SETTLED_SPIN_RAD_S
}

/// The car's total spin rate, in rad/s.
fn spin_rad_s(world: &World) -> f32 {
    let [x, y, z] = world.angular_velocity_rad_s();
    (x * x + y * y + z * z).sqrt()
}

// ---------------------------------------------------------------------------------------
// Playground ramps
// ---------------------------------------------------------------------------------------

/// Every playground ramp at every test speed, in Realistic mode: once with the nozzles
/// off (ballistic airtime), once with PD (the air a landing costs).
fn measure_ramps(setup: &Setup) -> Result<Vec<RampResult>, Box<dyn Error>> {
    let (_, realistic) = setup.modes[0];
    let model = spin_test_world(setup, realistic, 0.0)?.car_model();
    let mut results = Vec::new();
    for (piece, spec) in setup.course.pieces.iter().enumerate() {
        let PieceSpec::Ramp {
            pos,
            yaw,
            length,
            angle_deg,
            ..
        } = spec
        else {
            continue;
        };
        for speed_m_s in RAMP_SPEEDS_M_S {
            let approach = Approach {
                foot_m: *pos,
                yaw_deg: *yaw,
                speed_m_s,
            };
            let off = run_ramp(setup, realistic, &approach, None)?;
            let mut pd = Pd::new(&model, &setup.controllers);
            let with_pd = run_ramp(setup, realistic, &approach, Some(&mut pd))?;
            results.push(RampResult {
                piece,
                length_m: *length,
                angle_deg: *angle_deg,
                speed_m_s,
                off,
                pd: with_pd,
            });
        }
    }
    Ok(results)
}

/// Where a ramp run starts: the ramp's foot, its heading and the cruise speed.
struct Approach {
    foot_m: [f32; 3],
    yaw_deg: f32,
    speed_m_s: f32,
}

/// What one ramp run saw of its first jump off the ramp.
#[derive(Default)]
struct JumpLog {
    takeoff_speed_m_s: f32,
    airtime_s: f32,
    /// Time in the air while the thruster interlock still cut the controller, in s.
    interlock_held_s: f32,
    /// Air used from take-off until `AFTER_LANDING_S` after touchdown, in kg.
    air_kg: f32,
    /// The crash cause, if the run crashed.
    crash: Option<String>,
}

/// Drives toward the ramp with cruise control and logs the first take-off past the ramp
/// foot (the drop onto the wheels right after placement is not a jump). `pd`: None flies
/// with the nozzles off.
fn run_ramp(
    setup: &Setup,
    mode: ModeParams,
    approach: &Approach,
    mut pd: Option<&mut Pd>,
) -> Result<JumpLog, Box<dyn Error>> {
    let mut world = World::new_on_course(
        &setup.vehicle,
        mode,
        &setup.course,
        DEFAULT_GROUND_FRICTION,
        0,
    );
    let yaw_rad = approach.yaw_deg.to_radians();
    // Heading of a piece with yaw ψ about +Y: (cos ψ, 0, −sin ψ).
    let heading = [yaw_rad.cos(), 0.0, -yaw_rad.sin()];
    let foot = approach.foot_m;
    let start = [
        foot[0] - heading[0] * APPROACH_M,
        START_HEIGHT_M,
        foot[2] - heading[2] * APPROACH_M,
    ];
    let rotation = [0.0, (yaw_rad / 2.0).sin(), 0.0, (yaw_rad / 2.0).cos()];
    let velocity = heading.map(|component| component * approach.speed_m_s);
    world.place_car(start, rotation, velocity, [0.0; 3])?;

    let mut log = JumpLog::default();
    let mut takeoff_s: Option<f32> = None;
    let mut landing_s: Option<f32> = None;
    let mut air_at_takeoff_kg = 0.0;
    let mut time_s = 0.0;
    while time_s < RAMP_RUN_TIMEOUT_S {
        let input = cruise_input(&world, approach.speed_m_s);
        if world.is_control_step() {
            if let Some(pd) = pd.as_deref_mut() {
                world.set_controller_throttles(pd.act(&ControlCtx::for_world(&world, input)));
            }
        }
        world.step(input);
        time_s += PHYSICS_DT_S;
        let past_foot = distance_along_m(&world, start, heading) > APPROACH_M;
        for event in world.drain_events() {
            match event {
                SimEvent::AirborneStart if past_foot && takeoff_s.is_none() => {
                    takeoff_s = Some(time_s);
                    log.takeoff_speed_m_s = world.forward_speed_m_s();
                    air_at_takeoff_kg = world.air_used_kg();
                }
                SimEvent::AirborneEnd if takeoff_s.is_some() && landing_s.is_none() => {
                    landing_s = Some(time_s);
                }
                SimEvent::Crash { cause } => log.crash = Some(format!("{cause:?}")),
                _ => {}
            }
        }
        if let (Some(takeoff_time_s), None) = (takeoff_s, landing_s) {
            log.airtime_s = time_s - takeoff_time_s;
            if world.thrusters_locked() {
                log.interlock_held_s += PHYSICS_DT_S;
            }
        }
        let settled_after_landing =
            landing_s.is_some_and(|landed| time_s - landed >= AFTER_LANDING_S);
        if settled_after_landing || log.crash.is_some() {
            break;
        }
    }
    log.air_kg = world.air_used_kg() - air_at_takeoff_kg;
    Ok(log)
}

/// How far the car has come from `start_m` along `heading`, in m.
fn distance_along_m(world: &World, start_m: [f32; 3], heading: [f32; 3]) -> f32 {
    let position = world.chassis_pose().position_m;
    (position[0] - start_m[0]) * heading[0] + (position[2] - start_m[2]) * heading[2]
}

/// Throttle that holds `speed_m_s` on the approach; steering straight.
fn cruise_input(world: &World, speed_m_s: f32) -> InputFrame {
    let error_m_s = speed_m_s - world.forward_speed_m_s();
    let throttle = (error_m_s * CRUISE_GAIN_PER_M_S).clamp(-1.0, 1.0);
    InputFrame {
        throttle: (throttle * 127.0) as i8,
        ..InputFrame::default()
    }
}

fn print_ramp_table(ramps: &[RampResult]) {
    println!(
        "\n=== playground ramps (realistic; airtime with nozzles off; landing air with PD) ==="
    );
    println!(
        "piece len m angle | speed m/s takeoff m/s | off: airtime s crash | PD: airtime s air kg interlock s crash"
    );
    for ramp in ramps {
        println!(
            "{:>5} {:>5.0} {:>5.0} | {:>9.0} {:>11.1} | {:>14.2} {:<10} | {:>13.2} {:>6.1} {:>11.2} {}",
            ramp.piece,
            ramp.length_m,
            ramp.angle_deg,
            ramp.speed_m_s,
            ramp.off.takeoff_speed_m_s,
            ramp.off.airtime_s,
            ramp.off.crash.as_deref().unwrap_or("-"),
            ramp.pd.airtime_s,
            ramp.pd.air_kg,
            ramp.pd.interlock_held_s,
            ramp.pd.crash.as_deref().unwrap_or("-")
        );
    }
}

fn flip_json(flip: &FlipResult) -> Value {
    json!({
        "catch_style": style_name(flip.style),
        "coast_rate_rad_s": flip.coast_rate_rad_s,
        "spin_up_s": flip.spin_up_s,
        "coast_s": flip.coast_s,
        "brake_s": flip.brake_s,
        "catch_to_landable_s": flip.catch_to_landable_s,
        "catch_to_settled_s": flip.catch_to_settled_s,
        "time_to_landable_s": flip.time_to_landable_s(),
        "spin_up_air_kg": flip.spin_up_air_kg,
        "brake_air_kg": flip.brake_air_kg,
        "catch_to_landable_air_kg": flip.catch_to_landable_air_kg,
        "air_to_landable_kg": flip.air_to_landable_kg(),
        "rotation_rad": flip.rotation_rad,
        "settled": flip.settled,
    })
}

fn ramp_json(ramp: &RampResult) -> Value {
    json!({
        "piece": ramp.piece,
        "length_m": ramp.length_m,
        "angle_deg": ramp.angle_deg,
        "speed_m_s": ramp.speed_m_s,
        "takeoff_speed_m_s": ramp.off.takeoff_speed_m_s,
        "airtime_off_s": ramp.off.airtime_s,
        "crash_off": ramp.off.crash,
        "airtime_pd_s": ramp.pd.airtime_s,
        "landing_air_pd_kg": ramp.pd.air_kg,
        "interlock_held_pd_s": ramp.pd.interlock_held_s,
        "crash_pd": ramp.pd.crash,
    })
}
