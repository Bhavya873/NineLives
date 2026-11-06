//! Stage F, the flight (SPEC §9.6): the driver bot rolls the car onto one strong flip
//! plate on a flat straight, so every episode turns the car over with the driver still on
//! the controls. Success is Stage A's (wheels down and quiet), and the landing is judged
//! for heading like Stage D's. `stage_start` applies these draws.

use rand::RngExt;
use serde::Deserialize;

use crate::chaos::random_direction;
use crate::course::CourseSpec;
use crate::course_gen::{FlipPlates, FlipPreset, PieceCatalog, pick_preset};
use crate::driving_stages::{straight_course, stream_rng};
use crate::scenarios::draw;
use crate::spin_axis::{ScenarioTag, SpinAxis};

/// The ChaCha8 stream for Stage F draws.
const FLIGHT_STAGE_RNG_STREAM: u64 = 10;
/// The ChaCha8 stream for the widened draws (`[widening.stage_f]`), apart from the stream
/// above so the legacy draws (and every seed's speed) stay as they were.
const FLIGHT_WIDENING_RNG_STREAM: u64 = 12;
/// The name of the roll + pitch plate the widened draws build.
pub const MIXED_PRESET_NAME: &str = "roll_flip";

/// Stage F, the flight (`[stage_f]`). Ranges are [low, high], drawn evenly.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlightStageConfig {
    /// Episodes are cut off after this long, in s.
    pub time_limit_s: f32,
    /// Speed the car meets the plate at, which the driver bot then holds, in m/s.
    pub speed_m_s: [f32; 2],
    /// Multiplies the preset's impulses (pieces.json `strong_strength` is a strong plate).
    pub strength: [f32; 2],
    /// How far ahead of the car's start the plate sits, in m.
    pub plate_ahead_m: f32,
    /// Half the width of the course's bounds, in m: room for a sideways landing.
    pub half_width_m: f32,
    /// Flat floor kept past the plate, in m.
    pub run_out_m: f32,
}

/// The widened Stage F draws (`[widening.stage_f]`): a wider strength range, plates that
/// roll and pitch at once, and a car that meets the plate slightly off level.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlightWidening {
    /// Replaces `[stage_f]`'s strength range.
    pub strength: [f32; 2],
    /// Chance of a mixed plate: a barrel roll and a flip added together.
    pub mixed_probability: f32,
    /// What the roll part of a mixed plate is multiplied by.
    pub mixed_roll_share: [f32; 2],
    /// What the flip part of a mixed plate is multiplied by.
    pub mixed_pitch_share: [f32; 2],
    /// The car starts tilted from level by up to this, about a random horizontal axis.
    pub start_tilt_deg: f32,
    /// ...and turned off its heading by up to this either way.
    pub start_yaw_deg: f32,
    /// ...and spinning about a random axis at up to this, in rad/s.
    pub start_spin_rad_s: f32,
}

/// How the car starts off level (widened Stage F only).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StartAttitude {
    /// Tilt from level, in rad, about the horizontal axis at `tilt_axis_rad` (from +X).
    pub tilt_rad: f32,
    pub tilt_axis_rad: f32,
    /// Turn off the heading, in rad.
    pub yaw_rad: f32,
    /// World-frame spin, in rad/s.
    pub spin_rad_s: [f32; 3],
}

/// One Stage F episode's draws.
#[derive(Debug, Clone, PartialEq)]
pub struct FlightSetup {
    /// The flip plate's preset (picked by its pieces.json weight).
    pub preset: FlipPreset,
    /// What the preset's impulses are multiplied by.
    pub strength: f32,
    /// The car's speed at the start, which the bot holds, in m/s.
    pub speed_m_s: f32,
    /// The widened start attitude; None starts the car level and not spinning.
    pub start: Option<StartAttitude>,
}

impl FlightSetup {
    /// What the eval records for this episode.
    pub fn tag(&self) -> ScenarioTag {
        let axis = if self.preset.name == MIXED_PRESET_NAME {
            Some(SpinAxis::Mixed)
        } else {
            SpinAxis::dominant(self.preset.angular_n_m_s)
        };
        ScenarioTag {
            preset: Some(self.preset.name.clone()),
            strength: Some(self.strength),
            spin_axis: axis,
        }
    }
}

/// A short plain label for a plate preset, for the viewer ("barrel L", "back flip").
pub fn preset_label(name: &str) -> String {
    match name {
        "barrel_left" => "barrel L".to_string(),
        "barrel_right" => "barrel R".to_string(),
        MIXED_PRESET_NAME => "roll + flip".to_string(),
        other => other.replace('_', " "),
    }
}

/// The draws for the Stage F episode with `seed`. With `widening` the preset, strength and
/// start attitude come from their own stream; the speed is the same either way.
pub fn draw_flight_setup(
    config: &FlightStageConfig,
    widening: Option<&FlightWidening>,
    plates: &FlipPlates,
    seed: u64,
) -> FlightSetup {
    let mut rng = stream_rng(seed, FLIGHT_STAGE_RNG_STREAM);
    let mut setup = FlightSetup {
        preset: pick_preset(&plates.presets, &mut rng).clone(),
        strength: draw(&mut rng, config.strength),
        speed_m_s: draw(&mut rng, config.speed_m_s),
        start: None,
    };
    if let Some(widening) = widening {
        widen(&mut setup, widening, plates, seed);
    }
    setup
}

/// Replaces `setup`'s preset, strength and start attitude with the widened draws.
fn widen(setup: &mut FlightSetup, widening: &FlightWidening, plates: &FlipPlates, seed: u64) {
    let mut rng = stream_rng(seed, FLIGHT_WIDENING_RNG_STREAM);
    setup.strength = draw(&mut rng, widening.strength);
    let wants_mixed = rng.random_range(0.0..1.0) < widening.mixed_probability;
    setup.preset = match wants_mixed.then(|| mixed_preset(plates, widening, &mut rng)) {
        Some(Some(mixed)) => mixed,
        _ => pick_preset(&plates.presets, &mut rng).clone(),
    };
    let tilt_rad = rng.random_range(0.0..=widening.start_tilt_deg.to_radians());
    let tilt_axis_rad = rng.random_range(0.0..std::f32::consts::TAU);
    let yaw_limit_rad = widening.start_yaw_deg.to_radians();
    let yaw_rad = rng.random_range(-yaw_limit_rad..=yaw_limit_rad);
    let spin = random_direction(&mut rng) * rng.random_range(0.0..=widening.start_spin_rad_s);
    setup.start = Some(StartAttitude {
        tilt_rad,
        tilt_axis_rad,
        yaw_rad,
        spin_rad_s: spin.to_array(),
    });
}

/// A barrel-roll preset and a flip preset added together, each scaled by its share; None
/// if pieces.json has no preset of one of the two kinds.
fn mixed_preset(
    plates: &FlipPlates,
    widening: &FlightWidening,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> Option<FlipPreset> {
    let of_kind = |roll: bool| -> Vec<FlipPreset> {
        let wanted = |preset: &&FlipPreset| {
            let [x, _, z] = preset.angular_n_m_s;
            preset.weight > 0.0
                && if roll {
                    x.abs() > z.abs()
                } else {
                    z.abs() > x.abs()
                }
        };
        plates.presets.iter().filter(wanted).cloned().collect()
    };
    let (rolls, flips) = (of_kind(true), of_kind(false));
    if rolls.is_empty() || flips.is_empty() {
        return None;
    }
    let roll = pick_preset(&rolls, rng);
    let flip = pick_preset(&flips, rng);
    let roll_share = draw(rng, widening.mixed_roll_share);
    let pitch_share = draw(rng, widening.mixed_pitch_share);
    let mix = |index: usize| {
        roll.angular_n_m_s[index] * roll_share + flip.angular_n_m_s[index] * pitch_share
    };
    Some(FlipPreset {
        name: MIXED_PRESET_NAME.to_string(),
        weight: 0.0,
        impulse_n_s: std::array::from_fn(|index| {
            (roll.impulse_n_s[index] + flip.impulse_n_s[index]) / 2.0
        }),
        angular_n_m_s: [mix(0), mix(1), mix(2)],
    })
}

/// A flat straight along +X from the origin with `setup`'s plate `plate_ahead_m` in
/// front of the start, `half_width_m` wide either side.
pub fn flight_course(
    config: &FlightStageConfig,
    catalog: &PieceCatalog,
    setup: &FlightSetup,
) -> CourseSpec {
    let plate_x_m = config.plate_ahead_m;
    let plate = setup
        .preset
        .kick_plate_at([plate_x_m, 0.0, 0.0], setup.strength);
    let end_x_m = plate_x_m + config.run_out_m;
    let mut course = straight_course("Stage F flight", catalog, end_x_m, vec![plate]);
    course.bounds.min[2] = -config.half_width_m;
    course.bounds.max[2] = config.half_width_m;
    course
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::course::PieceSpec;
    use crate::scenarios::{DEFAULT_SCENARIOS_TOML, ScenariosConfig};

    fn config() -> FlightStageConfig {
        ScenariosConfig::from_toml_str(DEFAULT_SCENARIOS_TOML)
            .unwrap()
            .stage_f
    }

    fn plates() -> FlipPlates {
        PieceCatalog::built_in().unwrap().flip_plates().unwrap()
    }

    #[test]
    fn the_same_seed_draws_the_same_flight() {
        let first = draw_flight_setup(&config(), None, &plates(), 42);
        let second = draw_flight_setup(&config(), None, &plates(), 42);

        assert_eq!(first, second);
    }

    #[test]
    fn draws_cover_every_weighted_preset_and_skip_the_flat_spin() {
        let mut seen = Vec::new();
        for seed in 0..300 {
            let setup = draw_flight_setup(&config(), None, &plates(), seed);
            if !seen.contains(&setup.preset.name) {
                seen.push(setup.preset.name);
            }
        }

        for preset in plates().presets {
            let wanted = preset.weight > 0.0;
            assert_eq!(seen.contains(&preset.name), wanted, "{}", preset.name);
        }
    }

    #[test]
    fn speed_and_strength_stay_in_their_ranges() {
        let config = config();
        for seed in 0..100 {
            let setup = draw_flight_setup(&config, None, &plates(), seed);

            let [slow, fast] = config.speed_m_s;
            let [weak, strong] = config.strength;
            assert!((slow..=fast).contains(&setup.speed_m_s));
            assert!((weak..=strong).contains(&setup.strength));
        }
    }

    #[test]
    fn the_course_has_one_plate_ahead_of_the_start_inside_wide_bounds() {
        let config = config();
        let setup = draw_flight_setup(&config, None, &plates(), 7);

        let course = flight_course(&config, PieceCatalog::built_in().unwrap(), &setup);

        assert_eq!(course.pieces.len(), 1);
        let PieceSpec::KickPlate { pos, impulse, .. } = course.pieces[0] else {
            panic!("not a kick plate: {:?}", course.pieces[0]);
        };
        assert_eq!(pos[0], config.plate_ahead_m);
        assert!(impulse[1] > 0.0, "the plate must throw the car up");
        assert_eq!(course.bounds.max[2], config.half_width_m);
        assert_eq!(course.bounds.min[2], -config.half_width_m);
    }
}
