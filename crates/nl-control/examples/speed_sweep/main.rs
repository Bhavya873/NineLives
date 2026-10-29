//! Speed and obstacle survival sweep (research R2c, plan 2026-10-27 §1.1): drives a car
//! straight onto one course piece at a time, at entry speeds from 2 to 55.7 m/s, with PD
//! flying the thrusters, and records how each run ended and why. Each crash is traced to
//! the contact that caused it (which part of the car, what it hit, where, along which
//! normal, how fast), with the suspension, landing and air numbers around it.
//!
//! cargo run --release -p nl-control --example speed_sweep -- \
//!     [--seeds 20] [--threads 6] [--vehicle path.toml] [--pieces ramp_15,bumps_0.4]
//!     [--speeds 10,30,55.6] [--out results/speed_sweep_before] [--runs-csv path.csv]
//!
//! Writes `<out>.json` (summary per piece, mode and speed, plus one example of each
//! crash) and `<out>.csv` (one row per piece, mode and speed). `--runs-csv` also writes
//! every run (large; keep it out of git).
//!
//! The car is placed already rolling, level and pointing along the course, so the bot's
//! steering (validated only to 30 m/s) plays no part: the entry is clean by design. Seed 0
//! enters dead centre; other seeds shift it inside the clean-approach limits (lateral
//! ±0.5 m, heading ±2°, start phase within one tick's travel).

use std::error::Error;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use nl_control::config::DEFAULT_CONTROLLERS_TOML;
use nl_control::{ControlCtx, Controller, build_controller};
use nl_sim::contacts::{ScrapeRule, is_underbody_scrape};
use nl_sim::conventions::{PHYSICS_DT_S, PHYSICS_HZ};
use nl_sim::course::{BoundsSpec, CourseSpec, PieceSpec, Side, StartSpec};
use nl_sim::course_build::{
    MIN_CONCAVE_RADIUS_M, OBSTACLE_USER_DATA, bump_spacing_m, ramp_lip_height_m,
    wall_ramp_total_length_m,
};
use nl_sim::course_gen::PieceCatalog;
use nl_sim::diagnostics::{ChassisContactPoint, ChassisPart};
use nl_sim::events::SimEvent;
use nl_sim::input::InputFrame;
use nl_sim::modes::{DEFAULT_MODES_TOML, ModeParams, ModesConfig};
use nl_sim::traversal::ApproachConfig;
use nl_sim::vehicle_config::{DEFAULT_VEHICLE_TOML, VehicleConfig};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};
use serde::Serialize;

mod args;
mod report;

use args::parse_args;
use report::{write_runs_csv, write_summary};

const STANDARD_GRAVITY_M_S2: f32 = 9.81;
/// Where every piece starts along +X, in m.
const PIECE_X_M: f32 = 60.0;
/// Entry speeds, in m/s: below the lowest "complete" speeds, then 10 to 55 in 2.5 steps,
/// then the 200 km/h edge (55.6) and just past it (55.7, logged only).
const LOW_SPEEDS_M_S: [f32; 3] = [2.0, 5.0, 9.9];
const EDGE_SPEEDS_M_S: [f32; 2] = [55.6, 55.7];
/// Longest a run may take, in s.
const RUN_LIMIT_S: f32 = 30.0;
/// A run is over once the car has sat on all four wheels, level, this long.
const SETTLED_S: f32 = 1.5;
const SETTLED_TILT_DEG: f32 = 20.0;
/// …or has stood still this long (stopped on or before the piece).
const STOPPED_S: f32 = 2.0;
const STOPPED_M_S: f32 = 0.3;
/// A flight shorter than this is a wheel skipping, not a jump, in s.
const MIN_FLIGHT_S: f32 = 0.25;
/// A wheel counts as bottomed out at this share of its travel (vehicle.toml's
/// hard-landing compression).
const BOTTOMED_SHARE: f32 = 0.95;
/// Clean-approach jitter for seeds other than 0 (plan §1.2 limits are wider).
const LATERAL_JITTER_M: f32 = 0.5;
const HEADING_JITTER_DEG: f32 = 2.0;
/// Plan §1.3 underbody test: the contact is this far inside the body's bottom face and
/// its normal within this angle of body down; the tilt is under the scrape limit.
const UNDERBODY_MARGIN_M: f32 = 0.05;
const UNDERBODY_NORMAL_DEG: f32 = 30.0;
const SCRAPE_TILT_DEG: f32 = 25.0;
/// A contact on the floor counts as on its top face above this height, in m (the sim's
/// `contacts::FLOOR_TOP_MIN_Y_M`).
const FLOOR_TOP_MIN_Y_M: f32 = -0.05;

/// One course piece to test.
struct PieceCase {
    name: String,
    pieces: Vec<PieceSpec>,
    /// Where the piece ends along +X, in m (past it, the run counts as cleared).
    end_x_m: f32,
}

/// One run to do.
#[derive(Clone)]
struct Job {
    case: usize,
    mode_name: &'static str,
    speed_m_s: f32,
    seed: u64,
    /// Print every step (`--trace`).
    trace: bool,
    floor: Floor,
}

/// A contact of the body or roof, in the terms of plan §1.3.
#[derive(Debug, Clone, Serialize)]
struct ContactRecord {
    part: ChassisPart,
    /// "floor", "piece_surface" (ramps, kickers, corkscrews, halfpipes), "bump" or "wall".
    hit: &'static str,
    point_body_m: [f32; 3],
    normal_body: [f32; 3],
    normal_world: [f32; 3],
    /// Speed into the other collider along the normal, from before the step, in m/s.
    normal_speed_m_s: f32,
    /// Below the body's lower face minus the margin, normal within 30° of body down.
    underbody: bool,
    /// All four scrape conditions hold (docs/contracts/crash-scrape.md): underbody, normal
    /// up, gentle (at most `scrape_max_normal_m_s`) and nearly level.
    scrape: bool,
    tilt_deg: f32,
    time_s: f32,
}

/// The main landing of a run (its longest flight).
#[derive(Debug, Clone, Serialize)]
struct LandingRecord {
    airtime_s: f32,
    /// Horizontal distance from take-off to touchdown, in m.
    distance_m: f32,
    /// Fall speed (down positive) and total speed just before touchdown, in m/s.
    fall_m_s: f32,
    speed_m_s: f32,
    /// Angle of the touching wheels' ground normal from vertical, in degrees.
    surface_slope_deg: f32,
    /// Where it touched down, along +X, in m.
    land_x_m: f32,
}

/// Everything recorded about one run.
#[derive(Debug, Clone, Serialize)]
struct RunRecord {
    piece: String,
    mode: &'static str,
    speed_m_s: f32,
    seed: u64,
    /// "survived", "crashed" or "timeout".
    outcome: &'static str,
    crash_cause: Option<String>,
    /// i concave_bottoming, ii landing_slam, iii bump_face, iv other_* (see `classify`).
    failure_class: Option<&'static str>,
    cleared: bool,
    /// Where the centre of mass was when the run ended, in m.
    end_x_m: f32,
    end_y_m: f32,
    time_s: f32,
    peak_compression: f32,
    bottoming_events: u32,
    /// Peak total suspension force over the car's weight, in g.
    peak_load_g: f32,
    /// Total suspension load on the first step the body touched anything, in g.
    load_at_first_body_contact_g: Option<f32>,
    landing: Option<LandingRecord>,
    air_used_kg: f32,
    peak_tilt_deg: f32,
    saves: u32,
    hard_landings: u32,
    /// The fastest body contact of the run (for survived runs: the scrape severity).
    worst_contact: Option<ContactRecord>,
    /// The fastest contact on the crash step.
    crash_contact: Option<ContactRecord>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    let vehicle_text = match &args.vehicle {
        Some(path) => std::fs::read_to_string(path)?,
        None => DEFAULT_VEHICLE_TOML.to_string(),
    };
    let vehicle = VehicleConfig::from_toml_str(&vehicle_text)?;
    let cases = select_cases(piece_cases()?, &args.pieces);
    if let Some(trace) = &args.trace {
        return trace_one(&vehicle, &cases, trace, args.floor);
    }
    let jobs = make_jobs(&cases, &args.speeds, args.seeds, args.floor);
    eprintln!("{} runs on {} threads", jobs.len(), args.threads);
    let records = run_all(&vehicle, &cases, &jobs, args.threads)?;
    if let Some(path) = &args.runs_csv {
        write_runs_csv(path, &records)?;
    }
    write_summary(&args.out, &records, &cases, &args)?;
    Ok(())
}

/// The pieces of plan R2c, each alone on a long flat course, laid out the way the
/// generator and the playground lay them (docs/contracts/ramp-curvature.md: a curved
/// piece is its entry radius times its lip angle long, radius `MIN_CONCAVE_RADIUS_M`).
fn piece_cases() -> Result<Vec<PieceCase>, Box<dyn Error>> {
    let x = PIECE_X_M;
    let mut cases = Vec::new();
    for angle_deg in [6.0_f32, 8.0] {
        let length_m = ramp_length_m(angle_deg);
        cases.push(PieceCase {
            name: format!("ramp_{angle_deg:.0}"),
            pieces: vec![ramp(x, length_m, angle_deg)],
            end_x_m: x + length_m,
        });
    }
    // course_gen's ramp_gap at its hardest: a 6° ramp (the lowest lip) and a 6 m gap
    // with its catch pit.
    let gap_angle_deg = 6.0;
    let gap_ramp_m = ramp_length_m(gap_angle_deg);
    cases.push(PieceCase {
        name: "ramp_gap".to_string(),
        pieces: vec![
            ramp(x, gap_ramp_m, gap_angle_deg),
            PieceSpec::Gap {
                pos: [x + gap_ramp_m, 0.0, 0.0],
                length: 6.0,
                pit_depth: 0.5,
                entry_height: ramp_lip_height_m(gap_ramp_m, gap_angle_deg),
            },
        ],
        end_x_m: x + gap_ramp_m + 6.0,
    });
    let kicker_m = ramp_length_m(8.0);
    cases.push(PieceCase {
        name: "kicker".to_string(),
        pieces: vec![PieceSpec::Kicker {
            pos: [x, 0.0, 0.0],
            yaw: 0.0,
            length: kicker_m,
            width: 4.0,
            angle_deg: 8.0,
            side: Side::Left,
        }],
        end_x_m: x + kicker_m,
    });
    for height_m in [0.2, 0.4, 0.5] {
        cases.push(PieceCase {
            name: format!("bumps_{height_m:.1}"),
            pieces: vec![PieceSpec::Bumps {
                pos: [x, 0.0, 0.0],
                yaw: 0.0,
                count: 4,
                height: height_m,
            }],
            end_x_m: x + 4.0 * bump_spacing_m(height_m),
        });
    }
    // The corkscrew's low edge is a parabola: length = radius · tan(angle).
    let corkscrew_m = MIN_CONCAVE_RADIUS_M * 8.0_f32.to_radians().tan();
    cases.push(PieceCase {
        name: "corkscrew".to_string(),
        pieces: vec![PieceSpec::Corkscrew {
            pos: [x, 0.0, 0.0],
            yaw: 0.0,
            length: corkscrew_m,
            width: 4.0,
            angle_deg: 8.0,
            bank_deg: 8.0,
            side: Side::Right,
        }],
        end_x_m: x + corkscrew_m,
    });
    cases.push(PieceCase {
        name: "halfpipe".to_string(),
        pieces: vec![PieceSpec::Halfpipe {
            pos: [x, 0.0, 0.0],
            yaw: 0.0,
            radius: 6.0,
            length: 20.0,
        }],
        end_x_m: x + 20.0,
    });
    // A table top the height of a 2 m wall: the ramp starts at PIECE_X_M.
    cases.push(PieceCase {
        name: "wall_ramp".to_string(),
        pieces: vec![PieceSpec::Wall {
            pos: [x, 0.0, 0.0],
            yaw: 0.0,
            height: 2.0,
            with_ramp: true,
        }],
        end_x_m: x + wall_ramp_total_length_m(2.0),
    });
    cases.extend(flip_plate_cases()?);
    Ok(cases)
}

/// The length of a ramp of lip angle `angle_deg` whose entry curve has radius
/// `MIN_CONCAVE_RADIUS_M`, in m.
fn ramp_length_m(angle_deg: f32) -> f32 {
    MIN_CONCAVE_RADIUS_M * angle_deg.to_radians()
}

/// One mild flip plate per preset that generated courses draw (weight > 0).
fn flip_plate_cases() -> Result<Vec<PieceCase>, Box<dyn Error>> {
    let plates = PieceCatalog::built_in()?
        .flip_plates()
        .ok_or("pieces.json has no flip_plate template")?;
    let mut cases = Vec::new();
    for preset in plates.presets.iter().filter(|preset| preset.weight > 0.0) {
        cases.push(PieceCase {
            name: format!("flip_{}", preset.name),
            pieces: vec![preset.kick_plate_at([PIECE_X_M, 0.0, 0.0], plates.strength[0])],
            end_x_m: PIECE_X_M + 4.0,
        });
    }
    Ok(cases)
}

fn ramp(x_m: f32, length_m: f32, angle_deg: f32) -> PieceSpec {
    PieceSpec::Ramp {
        pos: [x_m, 0.0, 0.0],
        yaw: 0.0,
        length: length_m,
        width: 8.0,
        angle_deg,
        v_complete_min_m_s: 0.0,
    }
}

/// Keeps the cases named in `wanted` (all of them when it is empty).
fn select_cases(cases: Vec<PieceCase>, wanted: &[String]) -> Vec<PieceCase> {
    if wanted.is_empty() {
        return cases;
    }
    cases
        .into_iter()
        .filter(|case| wanted.contains(&case.name))
        .collect()
}

/// The default speed list (see `LOW_SPEEDS_M_S`).
fn default_speeds_m_s() -> Vec<f32> {
    let mut speeds = LOW_SPEEDS_M_S.to_vec();
    for step in 0..=18 {
        speeds.push(10.0 + 2.5 * step as f32);
    }
    speeds.extend(EDGE_SPEEDS_M_S);
    speeds
}

fn make_jobs(cases: &[PieceCase], speeds_m_s: &[f32], seeds: u64, floor: Floor) -> Vec<Job> {
    let mut jobs = Vec::new();
    for case in 0..cases.len() {
        for mode_name in ["realistic", "arcade"] {
            for speed_m_s in speeds_m_s {
                for seed in 0..seeds {
                    jobs.push(Job {
                        case,
                        mode_name,
                        speed_m_s: *speed_m_s,
                        seed,
                        trace: false,
                        floor,
                    });
                }
            }
        }
    }
    jobs
}

/// Runs one job, `piece:mode:speed:seed`, printing every step and then its record.
fn trace_one(
    vehicle: &VehicleConfig,
    cases: &[PieceCase],
    spec: &str,
    floor: Floor,
) -> Result<(), Box<dyn Error>> {
    let parts: Vec<&str> = spec.split(':').collect();
    let [piece, mode_name, speed, seed] = parts[..] else {
        return Err("--trace needs piece:mode:speed:seed".into());
    };
    let case = cases
        .iter()
        .position(|case| case.name == piece)
        .ok_or_else(|| format!("--trace: no piece named {piece}"))?;
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)?;
    let job = Job {
        case,
        mode_name: if mode_name == "arcade" {
            "arcade"
        } else {
            "realistic"
        },
        speed_m_s: speed.parse()?,
        seed: seed.parse()?,
        trace: true,
        floor,
    };
    let mode = mode_by_name(&modes, job.mode_name);
    let record = run_one(vehicle, mode, &cases[case], &job)?;
    println!("{}", serde_json::to_string_pretty(&record)?);
    Ok(())
}

/// Runs every job on `threads` worker threads; results come back in job order.
fn run_all(
    vehicle: &VehicleConfig,
    cases: &[PieceCase],
    jobs: &[Job],
    threads: usize,
) -> Result<Vec<RunRecord>, Box<dyn Error>> {
    let modes = ModesConfig::from_toml_str(DEFAULT_MODES_TOML)?;
    let next_job = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<RunRecord>>> = Mutex::new(vec![None; jobs.len()]);
    let failure: Mutex<Option<String>> = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let index = next_job.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = jobs.get(index) else { break };
                    let mode = mode_by_name(&modes, job.mode_name);
                    match run_one(vehicle, mode, &cases[job.case], job) {
                        Ok(record) => {
                            if let Ok(mut results) = results.lock() {
                                results[index] = Some(record);
                            }
                        }
                        Err(error) => {
                            if let Ok(mut failure) = failure.lock() {
                                *failure = Some(error.to_string());
                            }
                        }
                    }
                    if index % 500 == 0 {
                        eprintln!("  {index}/{}", jobs.len());
                    }
                }
            });
        }
    });
    if let Some(error) = failure.into_inner().map_err(|_| "a worker panicked")? {
        return Err(error.into());
    }
    let results = results.into_inner().map_err(|_| "a worker panicked")?;
    Ok(results.into_iter().flatten().collect())
}

fn mode_by_name(modes: &ModesConfig, name: &str) -> ModeParams {
    if name == "arcade" {
        modes.arcade
    } else {
        modes.realistic
    }
}

/// The course's floor (its bounds' footprint): from x = -`behind_m` to `ahead_m`, and
/// ±`half_width_m` across, in m. Its size matters: see the floor-size note in
/// docs/research/speed-survival.md.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Floor {
    pub behind_m: f32,
    pub ahead_m: f32,
    pub half_width_m: f32,
    /// If above 0, the floor is cut into boxes this long along +X (zero-length gaps,
    /// which the course loader refuses but `build_course_shapes` accepts), in m.
    pub tile_m: f32,
}

/// A long flat course with the case's pieces; tall bounds so only falling into a gap
/// (or driving off the floor) counts as out of bounds.
fn course_for(case: &PieceCase, floor: Floor) -> CourseSpec {
    let mut pieces = case.pieces.clone();
    if floor.tile_m > 0.0 {
        let mut seam_x_m = -floor.behind_m + floor.tile_m;
        while seam_x_m < floor.ahead_m {
            pieces.push(PieceSpec::Gap {
                pos: [seam_x_m, 0.0, 0.0],
                length: 0.0,
                pit_depth: 0.0,
                entry_height: 0.0,
            });
            seam_x_m += floor.tile_m;
        }
    }
    CourseSpec {
        name: case.name.clone(),
        start: StartSpec {
            pos: [0.0, 0.7, 0.0],
            yaw: 0.0,
        },
        bounds: BoundsSpec {
            min: [-floor.behind_m, -20.0, -floor.half_width_m],
            max: [floor.ahead_m, 300.0, floor.half_width_m],
        },
        pieces,
    }
}

/// Small deterministic numbers in [-1, 1] for a seed (splitmix64), for the entry jitter.
fn jitter(seed: u64, salt: u64) -> f32 {
    let mut z = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(salt.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 23) as f32 - 1.0
}

/// Builds the world and puts the car on the approach, rolling at the job's speed.
fn start_run(vehicle: &VehicleConfig, mode: ModeParams, case: &PieceCase, job: &Job) -> World {
    let course = course_for(case, job.floor);
    let mut world = World::new_on_course(vehicle, mode, &course, DEFAULT_GROUND_FRICTION, job.seed);
    // Settle on the wheels first, to start from the resting ride height.
    for _ in 0..PHYSICS_HZ {
        world.step(InputFrame::default());
    }
    let resting_y_m = world.chassis_pose().position_m[1];
    let (lateral_m, heading_rad, phase) = if job.seed == 0 {
        (0.0, 0.0, 0.0)
    } else {
        (
            LATERAL_JITTER_M * jitter(job.seed, 1),
            HEADING_JITTER_DEG.to_radians() * jitter(job.seed, 2),
            0.5 * (jitter(job.seed, 3) + 1.0),
        )
    };
    let speed = job.speed_m_s;
    // Half a second of run-up, plus a fraction of one tick's travel (contact phase).
    let lead_m = 10.0 + 0.5 * speed + phase * speed * PHYSICS_DT_S;
    let half_yaw = 0.5 * heading_rad;
    let rotation = [0.0, libm::sinf(half_yaw), 0.0, libm::cosf(half_yaw)];
    // Yaw turns the nose towards -Z for positive angles (SPEC §4).
    let velocity = [
        speed * libm::cosf(heading_rad),
        0.0,
        -speed * libm::sinf(heading_rad),
    ];
    // The lateral jitter is the offset at the piece's foot, so start far enough over to
    // drift into it along the heading.
    let start_z_m = lateral_m + lead_m * libm::tanf(heading_rad);
    let position = [PIECE_X_M - lead_m, resting_y_m, start_z_m];
    // The values are finite and the rotation is a unit quaternion, so this cannot fail.
    let _ = world.place_car(position, rotation, velocity, [0.0; 3]);
    world.refill_tank();
    world
}

/// Throttle that holds `target_m_s` while on the wheels (steering is left centred).
fn driver_input(world: &World, target_m_s: f32) -> InputFrame {
    let error_m_s = target_m_s - world.forward_speed_m_s();
    let fraction = (error_m_s * 0.5).clamp(-1.0, 1.0);
    InputFrame {
        throttle: (fraction * 127.0).round() as i8,
        ..InputFrame::default()
    }
}

/// The chassis motion just before a step, for contact speeds.
struct MotionBefore {
    velocity_m_s: [f32; 3],
    spin_rad_s: [f32; 3],
    centre_m: [f32; 3],
}

impl MotionBefore {
    fn of(world: &World) -> MotionBefore {
        MotionBefore {
            velocity_m_s: world.linear_velocity_m_s(),
            spin_rad_s: world.angular_velocity_rad_s(),
            centre_m: world.chassis_pose().position_m,
        }
    }

    /// Speed of the body point at `point_m` along `normal`, in m/s.
    fn speed_along(&self, point_m: [f32; 3], normal: [f32; 3]) -> f32 {
        let arm = sub(point_m, self.centre_m);
        let point_velocity = add(self.velocity_m_s, cross(self.spin_rad_s, arm));
        dot(point_velocity, normal)
    }
}

/// One line per step for `--trace`: time, position, fall speed, tilt, wheel
/// compressions (m), and every body contact (body-frame point, world normal, speed).
fn print_trace_step(world: &World, before: &MotionBefore, time_s: f32) {
    let [x, y, z] = world.chassis_pose().position_m;
    let mut line = format!(
        "t={time_s:.3} x={x:.2} y={y:.2} z={z:.2} vy={:.2} w={:?} tilt={:.1}",
        world.linear_velocity_m_s()[1],
        world
            .angular_velocity_rad_s()
            .map(|w| (w * 10.0).round() / 10.0),
        world.tilt_rad().to_degrees()
    );
    for wheel in world.wheel_diagnostics() {
        line.push_str(&format!(" {:.3}", wheel.compression_m));
    }
    let pose = world.chassis_pose();
    let inverse = conjugate(pose.rotation_xyzw);
    for point in world.chassis_contact_points() {
        let [px, py, pz] = rotate(inverse, sub(point.point_m, pose.position_m));
        let [nx, ny, nz] = point.normal;
        let speed = before.speed_along(point.point_m, point.normal);
        line.push_str(&format!(
            " | {} p=({px:.2},{py:.2},{pz:.2}) n=({nx:.2},{ny:.2},{nz:.2}) v={speed:.2}",
            hit_name(&point)
        ));
    }
    println!("{line}");
}

/// Mutable state of one run while it is being stepped.
struct RunState {
    record: RunRecord,
    flight_start: Option<(f32, [f32; 3])>,
    longest_flight_s: f32,
    bottomed_last_step: bool,
    settled_s: f32,
    stopped_s: f32,
    flown: bool,
}

/// Drives one run to its end and returns its record.
fn run_one(
    vehicle: &VehicleConfig,
    mode: ModeParams,
    case: &PieceCase,
    job: &Job,
) -> Result<RunRecord, Box<dyn Error>> {
    let mut world = start_run(vehicle, mode, case, job);
    let mut controller = build_controller("pd", vehicle, mode, DEFAULT_CONTROLLERS_TOML, None)?;
    controller.reset(job.seed);
    let weight_n = vehicle.chassis.dry_mass_kg * STANDARD_GRAVITY_M_S2;
    let max_force_n = vehicle.wheels.max_suspension_force_n;
    let travel_m = vehicle.wheels.suspension_max_travel_m;
    let half_height_m = vehicle.chassis.half_extents_m[1];
    let air_at_start_kg = world.air_used_kg();
    let mut state = RunState::new(case, job);
    let limit_steps = (RUN_LIMIT_S * PHYSICS_HZ as f32) as usize;
    for step in 0..limit_steps {
        let input = driver_input(&world, job.speed_m_s);
        if world.is_control_step() {
            let ctx = ControlCtx::for_world(&world, input);
            world.set_controller_throttles(controller.act(&ctx));
        }
        let before = MotionBefore::of(&world);
        world.step(input);
        let time_s = (step + 1) as f32 * PHYSICS_DT_S;
        state.record.time_s = time_s;
        state.track_suspension(&world, weight_n, max_force_n, travel_m);
        state.track_contacts(
            &world,
            &before,
            half_height_m,
            time_s,
            weight_n,
            max_force_n,
        );
        if job.trace {
            print_trace_step(&world, &before, time_s);
        }
        let crashed = state.read_events(&mut world, &before, time_s);
        state.record.peak_tilt_deg = state
            .record
            .peak_tilt_deg
            .max(world.tilt_rad().to_degrees());
        if crashed {
            state.record.outcome = "crashed";
            state.record.crash_contact = fastest_contact(&world, &before, half_height_m, time_s);
            break;
        }
        if state.is_over(&world, case) {
            state.record.outcome = "survived";
            break;
        }
    }
    let [end_x_m, end_y_m, _] = world.chassis_pose().position_m;
    state.record.end_x_m = end_x_m;
    state.record.end_y_m = end_y_m;
    state.record.cleared = state.record.end_x_m > case.end_x_m;
    state.record.air_used_kg = world.air_used_kg() - air_at_start_kg;
    state.record.failure_class = classify(&state.record);
    Ok(state.record)
}

impl RunState {
    fn new(case: &PieceCase, job: &Job) -> RunState {
        RunState {
            record: RunRecord {
                piece: case.name.clone(),
                mode: job.mode_name,
                speed_m_s: job.speed_m_s,
                seed: job.seed,
                outcome: "timeout",
                crash_cause: None,
                failure_class: None,
                cleared: false,
                end_x_m: 0.0,
                end_y_m: 0.0,
                time_s: 0.0,
                peak_compression: 0.0,
                bottoming_events: 0,
                peak_load_g: 0.0,
                load_at_first_body_contact_g: None,
                landing: None,
                air_used_kg: 0.0,
                peak_tilt_deg: 0.0,
                saves: 0,
                hard_landings: 0,
                worst_contact: None,
                crash_contact: None,
            },
            flight_start: None,
            longest_flight_s: 0.0,
            bottomed_last_step: false,
            settled_s: 0.0,
            stopped_s: 0.0,
            flown: false,
        }
    }

    /// Peak compression, bottoming onsets and peak suspension load.
    fn track_suspension(&mut self, world: &World, weight_n: f32, max_force_n: f32, travel_m: f32) {
        let mut bottomed = false;
        for wheel in world.wheel_diagnostics() {
            if !wheel.in_contact {
                continue;
            }
            let share = (wheel.compression_m / travel_m).clamp(0.0, 1.0);
            self.record.peak_compression = self.record.peak_compression.max(share);
            bottomed |= share >= BOTTOMED_SHARE;
        }
        if bottomed && !self.bottomed_last_step {
            self.record.bottoming_events += 1;
        }
        self.bottomed_last_step = bottomed;
        let load_g = suspension_load_g(world, weight_n, max_force_n);
        self.record.peak_load_g = self.record.peak_load_g.max(load_g);
    }

    /// The fastest body contact so far, and the load when the body first touched.
    fn track_contacts(
        &mut self,
        world: &World,
        before: &MotionBefore,
        half_height_m: f32,
        time_s: f32,
        weight_n: f32,
        max_force_n: f32,
    ) {
        let Some(contact) = fastest_contact(world, before, half_height_m, time_s) else {
            return;
        };
        if self.record.load_at_first_body_contact_g.is_none() {
            self.record.load_at_first_body_contact_g =
                Some(suspension_load_g(world, weight_n, max_force_n));
        }
        let faster = match &self.record.worst_contact {
            Some(worst) => contact.normal_speed_m_s > worst.normal_speed_m_s,
            None => true,
        };
        if faster {
            self.record.worst_contact = Some(contact);
        }
    }

    /// Reads this step's events; true on a crash.
    fn read_events(&mut self, world: &mut World, before: &MotionBefore, time_s: f32) -> bool {
        let mut crashed = false;
        for event in world.drain_events() {
            match event {
                SimEvent::AirborneStart => {
                    self.flight_start = Some((time_s, world.chassis_pose().position_m));
                }
                SimEvent::AirborneEnd => self.end_flight(world, before, time_s),
                SimEvent::Save { .. } => self.record.saves += 1,
                SimEvent::HardLanding { .. } => self.record.hard_landings += 1,
                SimEvent::Crash { cause } => {
                    self.record.crash_cause = Some(format!("{cause:?}"));
                    crashed = true;
                }
                _ => {}
            }
        }
        crashed
    }

    /// Records the landing if this flight is the longest so far.
    fn end_flight(&mut self, world: &World, before: &MotionBefore, time_s: f32) {
        let Some((start_s, start_m)) = self.flight_start.take() else {
            return;
        };
        let airtime_s = time_s - start_s;
        if airtime_s < MIN_FLIGHT_S {
            return;
        }
        self.flown = true;
        if airtime_s <= self.longest_flight_s {
            return;
        }
        self.longest_flight_s = airtime_s;
        let land_m = world.chassis_pose().position_m;
        let [vx, vy, vz] = before.velocity_m_s;
        self.record.landing = Some(LandingRecord {
            airtime_s,
            distance_m: (land_m[0] - start_m[0]).hypot(land_m[2] - start_m[2]),
            fall_m_s: -vy,
            speed_m_s: (vx * vx + vy * vy + vz * vz).sqrt(),
            surface_slope_deg: touching_slope_deg(world),
            land_x_m: land_m[0],
        });
    }

    /// True once the car has settled past the piece, or stopped anywhere.
    fn is_over(&mut self, world: &World, case: &PieceCase) -> bool {
        let level = world.tilt_rad().to_degrees() < SETTLED_TILT_DEG;
        let past_piece = world.chassis_pose().position_m[0] > case.end_x_m;
        if world.wheels_touching_count() == 4 && level && past_piece {
            self.settled_s += PHYSICS_DT_S;
        } else {
            self.settled_s = 0.0;
        }
        let [vx, vy, vz] = world.linear_velocity_m_s();
        if (vx * vx + vy * vy + vz * vz).sqrt() < STOPPED_M_S && level {
            self.stopped_s += PHYSICS_DT_S;
        } else {
            self.stopped_s = 0.0;
        }
        self.settled_s >= SETTLED_S || self.stopped_s >= STOPPED_S
    }
}

/// Total suspension force (each wheel capped at the max force) over the weight, in g.
fn suspension_load_g(world: &World, weight_n: f32, max_force_n: f32) -> f32 {
    let mut total_n = 0.0;
    for wheel in world.wheel_diagnostics() {
        if wheel.in_contact {
            total_n += wheel.suspension_force_n.clamp(0.0, max_force_n);
        }
    }
    total_n / weight_n
}

/// Mean slope under the touching wheels, in degrees from level.
fn touching_slope_deg(world: &World) -> f32 {
    let mut sum_deg = 0.0;
    let mut count = 0;
    for wheel in world.wheel_diagnostics() {
        if wheel.in_contact {
            sum_deg += libm::acosf(wheel.ground_normal[1].clamp(-1.0, 1.0)).to_degrees();
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        sum_deg / count as f32
    }
}

/// The body or roof contact with the highest normal speed this step, if any.
fn fastest_contact(
    world: &World,
    before: &MotionBefore,
    half_height_m: f32,
    time_s: f32,
) -> Option<ContactRecord> {
    let mut best: Option<ContactRecord> = None;
    for point in world.chassis_contact_points() {
        let record = contact_record(world, &point, before, half_height_m, time_s);
        let faster = match &best {
            Some(best) => record.normal_speed_m_s > best.normal_speed_m_s,
            None => true,
        };
        if faster {
            best = Some(record);
        }
    }
    best
}

/// One contact point in body-frame terms, with its plan §1.3 underbody test and the
/// scrape rule. The floor's normal is its true one (straight up), as in the sim's own
/// crash rule (`contacts::touching_points`).
fn contact_record(
    world: &World,
    point: &ChassisContactPoint,
    before: &MotionBefore,
    half_height_m: f32,
    time_s: f32,
) -> ContactRecord {
    let pose = world.chassis_pose();
    let inverse = conjugate(pose.rotation_xyzw);
    let point_body_m = rotate(inverse, sub(point.point_m, pose.position_m));
    let on_floor_top = point.other_shape == "cuboid"
        && point.other_user_data == 0
        && point.point_m[1] > FLOOR_TOP_MIN_Y_M;
    let normal_world = if on_floor_top {
        [0.0, -1.0, 0.0]
    } else {
        point.normal
    };
    let normal_body = rotate(inverse, normal_world);
    let tilt_deg = world.tilt_rad().to_degrees();
    // Normal from the car into the ground points down the body (-Y) for an underbody hit.
    let normal_down_deg = libm::acosf((-normal_body[1]).clamp(-1.0, 1.0)).to_degrees();
    let underbody = point_body_m[1] < -(half_height_m - UNDERBODY_MARGIN_M)
        && normal_down_deg <= UNDERBODY_NORMAL_DEG
        && tilt_deg < SCRAPE_TILT_DEG;
    let normal_speed_m_s = before.speed_along(point.point_m, normal_world).max(0.0);
    let rule = scrape_rule(half_height_m);
    let scrape = point.part == ChassisPart::Body
        && is_underbody_scrape(
            point_body_m,
            normal_body,
            normal_speed_m_s,
            tilt_deg.to_radians(),
            &rule,
        );
    ContactRecord {
        part: point.part,
        hit: hit_name(point),
        point_body_m,
        normal_body,
        normal_world,
        normal_speed_m_s,
        underbody,
        scrape,
        tilt_deg,
        time_s,
    }
}

/// The sim's scrape rule for a car with a body of `half_height_m`, from approach.toml.
fn scrape_rule(half_height_m: f32) -> ScrapeRule {
    let approach = ApproachConfig::built_in();
    ScrapeRule {
        body_half_height_m: half_height_m,
        underbody_margin_m: approach.scrape_underbody_margin_m,
        normal_limit_rad: approach.scrape_normal_deg.to_radians(),
        max_normal_m_s: approach.scrape_max_normal_m_s,
        max_tilt_rad: approach.scrape_max_tilt_deg.to_radians(),
    }
}

/// What the car touched, from the collider's tag and shape.
fn hit_name(point: &ChassisContactPoint) -> &'static str {
    let obstacle = point.other_user_data == OBSTACLE_USER_DATA;
    match (obstacle, point.other_shape) {
        (true, "convex") => "bump",
        (true, _) => "wall",
        (false, "trimesh") => "piece_surface",
        (false, "cuboid") => "floor",
        _ => "other",
    }
}

/// The failure class of a crashed run (plan R2c): (i) concave_bottoming — the body hit
/// the piece's own surface before any flight; (ii) landing_slam — the body hit the floor
/// or a piece after a flight; (iii) bump_face; (iv) other_* — a wall face, falling into
/// a gap, leaving the floor sideways, a tumble (upside down or stuck), a floor contact
/// with a solver-made normal, or anything else.
fn classify(record: &RunRecord) -> Option<&'static str> {
    let cause = record.crash_cause.as_deref()?;
    match cause {
        "OutOfBounds" if record.end_y_m < 0.0 => return Some("other_fell_in_gap"),
        "OutOfBounds" => return Some("other_left_course"),
        "UpsideDown" | "Stuck" => return Some("other_tumble"),
        _ => {}
    }
    let Some(contact) = &record.crash_contact else {
        return Some("other_unknown");
    };
    let flown = record.landing.is_some();
    // The floor is flat, so a true floor contact pushes straight up; a normal tipped more
    // than about 6° from vertical is the solver's, not the geometry's (see the
    // floor-contact note in docs/research/speed-survival.md).
    let tipped_floor_normal = contact.hit == "floor" && contact.normal_world[1] > -0.995;
    let class = match contact.hit {
        "bump" => "bump_face",
        "wall" => "other_wall_face",
        _ if tipped_floor_normal => "other_floor_contact_artifact",
        _ if flown => "landing_slam",
        "piece_surface" => "concave_bottoming",
        _ => "other_impact",
    };
    Some(class)
}

// Small vector helpers (quaternions are x, y, z, w).

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn conjugate(q: [f32; 4]) -> [f32; 4] {
    [-q[0], -q[1], -q[2], q[3]]
}

/// Rotates `v` by the unit quaternion `q`: v + 2w(u×v) + 2u×(u×v), with u = q.xyz.
fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let u = [q[0], q[1], q[2]];
    let uv = cross(u, v);
    let uuv = cross(u, uv);
    [
        v[0] + 2.0 * (q[3] * uv[0] + uuv[0]),
        v[1] + 2.0 * (q[3] * uv[1] + uuv[1]),
        v[2] + 2.0 * (q[3] * uv[2] + uuv[2]),
    ]
}
