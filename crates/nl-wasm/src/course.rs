//! Courses in the browser (SPEC §7.11, M7): `Game::load_course` rebuilds the world on a
//! course and hands the renderer the course's shapes, the same list the colliders were
//! built from, so what the player sees is what the car hits.

use nl_control::Controller;
use nl_sim::course::CourseSpec;
use nl_sim::course_build::build_course_shapes;
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};
use wasm_bindgen::prelude::*;

use crate::Game;

#[wasm_bindgen]
impl Game {
    /// Starts a fresh run on the course in `course_json` (the text of a `courses/*.json`
    /// file): the car at the start with a full tank, the clock, saves and crashes back
    /// at 0, the same mode and controller. Returns the course's shapes as JSON, a list
    /// of `{piece, surface, geometry, pose, solid}` (see `nl_sim::course_build`).
    /// Throws, naming the piece and field, if the course is invalid.
    pub fn load_course(&mut self, course_json: &str) -> Result<String, JsError> {
        let course = CourseSpec::from_json_str(course_json)?;
        let world = World::new_on_course(
            &self.vehicle,
            self.world.mode(),
            &course,
            DEFAULT_GROUND_FRICTION,
            self.seed,
        );
        self.start_fresh(world);
        Ok(serde_json::to_string(&build_course_shapes(&course))?)
    }

    /// Goes back to the open flat arena, as at start-up (keeping mode and controller).
    pub fn load_flat_ground(&mut self) {
        let world = World::new_on_flat_ground(
            &self.vehicle,
            self.world.mode(),
            DEFAULT_GROUND_FRICTION,
            self.seed,
        );
        self.start_fresh(world);
    }
}

impl Game {
    /// Gives the world the game's wind setting.
    pub(crate) fn apply_wind_level(&mut self) {
        let level = self.scenarios.wind.level(self.wind_level);
        self.world.set_wind_level(level);
    }

    /// Swaps in a new world and clears everything counted on the old one; the wind
    /// setting carries over.
    fn start_fresh(&mut self, mut world: World) {
        world.enable_auto_respawn(self.vehicle.events.respawn_delay_s);
        self.world = world;
        self.apply_wind_level();
        self.controller.reset(self.seed);
        self.buttons_since_control_step = 0;
        self.saves = 0;
        self.crashes = 0;
    }
}
