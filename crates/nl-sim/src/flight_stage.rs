//! Stage F, the flight (SPEC §9.6): the driver bot rolls the car onto one strong flip
//! plate on a flat straight, so every episode turns the car over with the driver still on
//! the controls. Success is Stage A's (wheels down and quiet), and the landing is judged
//! for heading like Stage D's. `stage_start` applies these draws.

use serde::Deserialize;

use crate::course::CourseSpec;
use crate::course_gen::{FlipPlates, FlipPreset, PieceCatalog};
use crate::driving_stages::{pick_weighted, straight_course, stream_rng};
use crate::scenarios::draw;

/// The ChaCha8 stream for Stage F draws.
const FLIGHT_STAGE_RNG_STREAM: u64 = 10;

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

/// One Stage F episode's draws.
#[derive(Debug, Clone, PartialEq)]
pub struct FlightSetup {
    /// The flip plate's preset (picked by its pieces.json weight).
    pub preset: FlipPreset,
    /// What the preset's impulses are multiplied by.
    pub strength: f32,
    /// The car's speed at the start, which the bot holds, in m/s.
    pub speed_m_s: f32,
}

/// The draws for the Stage F episode with `seed`.
pub fn draw_flight_setup(
    config: &FlightStageConfig,
    plates: &FlipPlates,
    seed: u64,
) -> FlightSetup {
    let mut rng = stream_rng(seed, FLIGHT_STAGE_RNG_STREAM);
    let weights: Vec<f32> = plates.presets.iter().map(|preset| preset.weight).collect();
    let indices: Vec<usize> = (0..plates.presets.len()).collect();
    let index = pick_weighted(&indices, &weights, &mut rng);
    FlightSetup {
        preset: plates.presets[index].clone(),
        strength: draw(&mut rng, config.strength),
        speed_m_s: draw(&mut rng, config.speed_m_s),
    }
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
        let first = draw_flight_setup(&config(), &plates(), 42);
        let second = draw_flight_setup(&config(), &plates(), 42);

        assert_eq!(first, second);
    }

    #[test]
    fn draws_cover_every_weighted_preset_and_skip_the_flat_spin() {
        let mut seen = Vec::new();
        for seed in 0..300 {
            let setup = draw_flight_setup(&config(), &plates(), seed);
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
            let setup = draw_flight_setup(&config, &plates(), seed);

            let [slow, fast] = config.speed_m_s;
            let [weak, strong] = config.strength;
            assert!((slow..=fast).contains(&setup.speed_m_s));
            assert!((weak..=strong).contains(&setup.strength));
        }
    }

    #[test]
    fn the_course_has_one_plate_ahead_of_the_start_inside_wide_bounds() {
        let config = config();
        let setup = draw_flight_setup(&config, &plates(), 7);

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
