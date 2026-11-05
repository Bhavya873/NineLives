//! Shared by the obstacle-landing test and example: drop an upright car onto car rows,
//! table tops, a landing pad and the ground beside a wall, in limited air, and say which
//! drops end on four wheels without a crash. Also the high-jump tank check (how much air
//! is left at touchdown after a jump to several apex heights).

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used, dead_code)]

use nl_control::config::{ControllersConfig, DEFAULT_CONTROLLERS_TOML};
use nl_control::{ControlCtx, Controller, Pd};
use nl_sim::conventions::{GRAVITY_M_S2, PHYSICS_DT_S};
use nl_sim::course::CourseSpec;
use nl_sim::events::SimEvent;
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModesConfig};
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// Where the centre of mass sits on its wheels, in m.
const REST_HEIGHT_M: f32 = 0.6;
/// A run stops this long after touchdown, in s, or at the limit.
const AFTER_LANDING_S: f32 = 4.0;
const RUN_LIMIT_S: f32 = 30.0;
/// Heights above the surface the drops start at, in m.
pub const DROP_HEIGHTS_M: [f32; 3] = [8.0, 20.0, 45.0];
/// Downward speed at the start, in m/s.
pub const DROP_SPEEDS_M_S: [f32; 3] = [0.0, 8.0, 16.0];
/// Sideways drift towards the aim point's +X, in m/s.
pub const DRIFT_SPEEDS_M_S: [f32; 2] = [0.0, 6.0];

/// One kind of landing surface and where on it the car should come down.
pub struct Surface {
    pub name: &'static str,
    /// The course piece, as course-file JSON.
    piece_json: String,
    /// Aim point on the surface, world x and z, m.
    pub aim_xz: [f32; 2],
    /// Surface height at the aim point, m.
    pub surface_y: f32,
}

/// The surfaces of the test table.
pub fn surfaces() -> Vec<Surface> {
    let mut list = Vec::new();
    for height in [1.0_f32, 1.6, 2.2] {
        // The third of five cars: x from 10.2 to 14.7.
        list.push(Surface {
            name: leak(format!("car row {height:.1} m")),
            piece_json: format!(
                r#"{{"type": "car_row", "pos": [0, 0, 0], "count": 5, "height": {height}}}"#
            ),
            aim_xz: [12.45, 0.0],
            surface_y: height,
        });
    }
    for height in [2.0_f32, 3.0] {
        // A table top: the ramp rises to the deck at `height` (x about 8 to 13 m).
        let (ramp_len, _) = nl_sim::course_build::wall_ramp_dims(height);
        list.push(Surface {
            name: leak(format!("table top {height:.1} m, ramp")),
            piece_json: format!(
                r#"{{"type": "wall", "pos": [0, 0, 0], "height": {height}, "with_ramp": true}}"#
            ),
            aim_xz: [ramp_len * 0.6, 0.0],
            // Close enough for the start height; the sim measures the real gap.
            surface_y: height * 0.35,
        });
    }
    list.push(Surface {
        name: "landing pad",
        piece_json: r#"{"type": "landing_pad", "pos": [0, 0, 0]}"#.to_string(),
        aim_xz: [10.0, 0.0],
        surface_y: 0.0,
    });
    list.push(Surface {
        name: "ground by wall 3 m",
        piece_json: r#"{"type": "wall", "pos": [0, 0, 0], "height": 3.0}"#.to_string(),
        aim_xz: [-4.0, 0.0],
        surface_y: 0.0,
    });
    list
}

/// Strings for the table, kept for the whole run.
fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

/// What one drop showed.
#[derive(Debug, Clone, Copy, Default)]
pub struct DropResult {
    pub crashed: bool,
    pub landed: bool,
    /// Air left in the tank at first touchdown, kg.
    pub air_at_touchdown_kg: f32,
    /// Air left at the end, kg.
    pub air_at_end_kg: f32,
    pub apex_m: f32,
}

impl DropResult {
    /// Four wheels down at the end and no crash.
    pub fn clean(&self) -> bool {
        self.landed && !self.crashed
    }
}

fn course(piece_json: &str) -> CourseSpec {
    let json = format!(
        r#"{{"version": 1, "name": "drops", "start": {{"pos": [0, 0.7, 0], "yaw": 0}},
        "bounds": {{"min": [-800, -20, -800], "max": [800, 400, 800]}}, "pieces": [{piece_json}]}}"#
    );
    CourseSpec::from_json_str(&json).unwrap()
}

/// A Pd for the default car in limited air (Realistic).
fn pd_and_world(course: &CourseSpec) -> (Pd, World) {
    let vehicle = VehicleConfig::from_toml_str(DEFAULT_VEHICLE_TOML).unwrap();
    let controllers = ControllersConfig::from_toml_str(DEFAULT_CONTROLLERS_TOML).unwrap();
    let mode = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)
        .unwrap()
        .realistic;
    let world = World::new_on_course(&vehicle, mode, course, DEFAULT_GROUND_FRICTION, 0);
    (Pd::for_vehicle(&vehicle, mode, &controllers), world)
}

/// Flies `world` with PD to the end of the drop.
fn fly(world: &mut World, pd: &mut Pd) -> DropResult {
    let mut result = DropResult::default();
    let mut first_touch_s: Option<f32> = None;
    let mut time_s = 0.0;
    pd.reset(0);
    while time_s < RUN_LIMIT_S && !result.crashed {
        if world.is_control_step() {
            let ctx = ControlCtx::for_world(world, InputFrame::default());
            world.set_controller_throttles(pd.act(&ctx));
        }
        world.step(InputFrame::default());
        time_s += PHYSICS_DT_S;
        result.apex_m = result.apex_m.max(world.chassis_pose().position_m[1]);
        if first_touch_s.is_none() && world.wheels_touching_count() > 0 {
            first_touch_s = Some(time_s);
            result.air_at_touchdown_kg = world.air_kg();
        }
        for event in world.drain_events() {
            if matches!(event, SimEvent::Crash { .. }) {
                result.crashed = true;
            }
        }
        if first_touch_s.is_some_and(|touched| time_s - touched >= AFTER_LANDING_S) {
            break;
        }
    }
    result.landed = world.wheels_touching_count() == 4;
    result.air_at_end_kg = world.air_kg();
    result
}

/// One drop onto `surface` from `height_m` above it, falling at `fall_m_s` and drifting
/// `drift_m_s` along +X, aimed so the ballistic path comes down on the aim point.
pub fn drop_onto(
    surface: &Surface,
    height_m: f32,
    fall_m_s: f32,
    drift_m_s: f32,
) -> DropResult {
    let course = course(&surface.piece_json);
    let (mut pd, mut world) = pd_and_world(&course);
    // h = v·t + g·t²/2, solved for t.
    let fall_time_s = (-fall_m_s
        + libm::sqrtf(fall_m_s * fall_m_s + 2.0 * GRAVITY_M_S2 * height_m))
        / GRAVITY_M_S2;
    let start = [
        surface.aim_xz[0] - drift_m_s * fall_time_s,
        surface.surface_y + REST_HEIGHT_M + height_m,
        surface.aim_xz[1],
    ];
    world
        .place_car(
            start,
            [0.0, 0.0, 0.0, 1.0],
            [drift_m_s, -fall_m_s, 0.0],
            [0.0; 3],
        )
        .unwrap();
    fly(&mut world, &mut pd)
}

/// A jump from flat ground with an apex `apex_m` above the start and `drift_m_s` forward.
pub fn high_jump(apex_m: f32, drift_m_s: f32) -> DropResult {
    let course = course(r#"{"type": "landing_pad", "pos": [0, 0, 0]}"#);
    let (mut pd, mut world) = pd_and_world(&course);
    let up_m_s = libm::sqrtf(2.0 * GRAVITY_M_S2 * apex_m);
    world
        .place_car(
            [0.0, 2.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [drift_m_s, up_m_s, 0.0],
            [0.0; 3],
        )
        .unwrap();
    fly(&mut world, &mut pd)
}

/// Every drop of the table: (surface name, height, fall speed, drift, result).
pub fn all_drops() -> Vec<(&'static str, f32, f32, f32, DropResult)> {
    let mut rows = Vec::new();
    for surface in surfaces() {
        for height in DROP_HEIGHTS_M {
            for fall in DROP_SPEEDS_M_S {
                for drift in DRIFT_SPEEDS_M_S {
                    rows.push((
                        surface.name,
                        height,
                        fall,
                        drift,
                        drop_onto(&surface, height, fall, drift),
                    ));
                }
            }
        }
    }
    rows
}

/// Prints the table by surface and the high-jump tank rows.
pub fn print_report(rows: &[(&'static str, f32, f32, f32, DropResult)]) -> (usize, usize) {
    println!("{:<26} {:>7} {:>7}", "surface", "clean", "of");
    let mut names: Vec<&str> = Vec::new();
    for row in rows {
        if !names.contains(&row.0) {
            names.push(row.0);
        }
    }
    for name in &names {
        let mine: Vec<_> = rows.iter().filter(|row| row.0 == *name).collect();
        let clean = mine.iter().filter(|row| row.4.clean()).count();
        println!("{name:<26} {clean:>7} {:>7}", mine.len());
    }
    let clean = rows.iter().filter(|row| row.4.clean()).count();
    println!("{:<26} {clean:>7} {:>7}", "all", rows.len());
    for row in rows.iter().filter(|row| !row.4.clean()) {
        println!(
            "  fail: {} h {} fall {} drift {} crashed {} landed {}",
            row.0, row.1, row.2, row.3, row.4.crashed, row.4.landed
        );
    }
    println!("high jumps (apex above start, forward 10 m/s): air at touchdown");
    for apex in [30.0_f32, 60.0, 100.0, 150.0, 220.0] {
        let jump = high_jump(apex, 10.0);
        println!(
            "  apex {apex:>5.0} m: tank {:>6.1} kg at touchdown, {:>6.1} kg at end, clean {}",
            jump.air_at_touchdown_kg,
            jump.air_at_end_kg,
            jump.clean()
        );
    }
    (clean, rows.len())
}
