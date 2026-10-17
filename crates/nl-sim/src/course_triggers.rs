//! Course triggers (SPEC §7.11): the checkpoint and finish lines and the kick plates.
//! Each one is a box standing on its marking. Every physics step the world asks which
//! boxes the car's centre of mass passed through since the last step (so a fast car
//! cannot step over a thin line) and acts on them: moves the respawn point, ends the
//! run, or pushes the car.

use rapier3d::prelude::Vector;
use serde::{Deserialize, Serialize};

use crate::course::{CourseSpec, PieceSpec};
use crate::course_build::{CourseShape, ShapeGeometry};
use crate::world::Pose;

/// How high above a checkpoint or finish line the car still counts as crossing it, in
/// m, so jumping over a line counts too.
const LINE_REACH_UP_M: f32 = 30.0;
/// How high above a kick plate the car counts as on it, in m: about the height of the
/// centre of mass when the wheels touch, plus some room for suspension travel.
const KICK_PLATE_REACH_UP_M: f32 = 1.5;
/// A checkpoint respawn puts the centre of mass this high above the line, in m: a
/// little above the car's resting height so it settles onto its wheels.
const RESPAWN_HEIGHT_M: f32 = 0.7;
/// How far below a marking still counts, in m (the centre of mass never gets there on
/// a floor, but a ramp-side marking may sit a little high).
const REACH_DOWN_M: f32 = 0.5;

/// What happens when the car enters a trigger.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TriggerAction {
    /// The `number`-th checkpoint (0 first, in file order). A respawn there puts the
    /// centre of mass at `respawn_position_m`, facing `yaw_rad`.
    Checkpoint {
        number: u32,
        respawn_position_m: [f32; 3],
        yaw_rad: f32,
    },
    /// The finish line.
    Finish,
    /// A kick plate: impulses in N·s and N·m·s, world frame (SPEC §7.11).
    KickPlate {
        piece: u32,
        impulse_n_s: [f32; 3],
        angular_n_m_s: [f32; 3],
    },
}

/// One trigger box and whether the car was inside it last step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Trigger {
    action: TriggerAction,
    /// The marking's centre and turn.
    pose: Pose,
    /// Half the marking's length (local X) and width (local Z), in m.
    half_length_m: f32,
    half_width_m: f32,
    reach_up_m: f32,
    /// False after firing, until the car leaves the marking's footprint; so a kick that
    /// throws the car up off its plate does not fire again when it lands back on it.
    armed: bool,
}

/// Every trigger of a course and how far the car has got. Empty on flat ground.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CourseTriggers {
    triggers: Vec<Trigger>,
    /// The car's centre of mass at the last update, in m; None right after a teleport.
    last_centre_of_mass_m: Option<[f32; 3]>,
    checkpoint_count: u32,
    /// The number of the next checkpoint that counts; lower ones are behind the car.
    next_checkpoint: u32,
    finished: bool,
}

impl CourseTriggers {
    /// The triggers of `course`, from its marking shapes (`build_course_shapes`).
    pub fn from_course(course: &CourseSpec, shapes: &[CourseShape]) -> CourseTriggers {
        let mut triggers = Vec::new();
        let mut checkpoint_count = 0;
        for shape in shapes {
            let (Some(piece_index), ShapeGeometry::Box { half_extents_m }) =
                (shape.piece, &shape.geometry)
            else {
                continue;
            };
            let piece = &course.pieces[piece_index];
            let Some((action, reach_up_m)) = trigger_action(piece_index, piece, checkpoint_count)
            else {
                continue;
            };
            if matches!(action, TriggerAction::Checkpoint { .. }) {
                checkpoint_count += 1;
            }
            triggers.push(Trigger {
                action,
                pose: shape.pose,
                half_length_m: half_extents_m[0],
                half_width_m: half_extents_m[2],
                reach_up_m,
                armed: true,
            });
        }
        CourseTriggers {
            triggers,
            last_centre_of_mass_m: None,
            checkpoint_count,
            next_checkpoint: 0,
            finished: false,
        }
    }

    /// Checks every trigger against the car's path since the last update and returns
    /// the ones that count this step: kick plates once per pass, checkpoints the first
    /// time in order, and the finish once.
    pub fn update(&mut self, centre_of_mass_m: [f32; 3]) -> Vec<TriggerAction> {
        let from_m = self.last_centre_of_mass_m.unwrap_or(centre_of_mass_m);
        self.last_centre_of_mass_m = Some(centre_of_mass_m);
        let mut hits = Vec::new();
        for index in 0..self.triggers.len() {
            let trigger = &mut self.triggers[index];
            let from = trigger.to_local(from_m);
            let to = trigger.to_local(centre_of_mass_m);
            let fired = trigger.armed && trigger.path_crosses(from, to);
            if fired {
                trigger.armed = false;
            }
            if !trigger.is_over_footprint(to) {
                trigger.armed = true;
            }
            let action = trigger.action;
            if fired && self.counts(action) {
                hits.push(action);
            }
        }
        hits
    }

    /// Forgets where the car was, after it is teleported: no path is traced from the
    /// old place, and every trigger fires again if the car lands in it. Progress
    /// (checkpoints reached, finished) is kept for the life of the world.
    pub fn forget_car(&mut self) {
        self.last_centre_of_mass_m = None;
        for trigger in &mut self.triggers {
            trigger.armed = true;
        }
    }

    /// Whether an entered trigger counts, updating the progress if so.
    fn counts(&mut self, action: TriggerAction) -> bool {
        match action {
            TriggerAction::Checkpoint { number, .. } => {
                if number < self.next_checkpoint {
                    return false;
                }
                self.next_checkpoint = number + 1;
                true
            }
            TriggerAction::Finish => {
                if self.finished {
                    return false;
                }
                self.finished = true;
                true
            }
            TriggerAction::KickPlate { .. } => true,
        }
    }

    /// True when the course has no triggers (always on flat ground).
    pub fn is_empty(&self) -> bool {
        self.triggers.is_empty()
    }

    /// How many checkpoints the course has.
    pub fn checkpoint_count(&self) -> u32 {
        self.checkpoint_count
    }

    /// How many checkpoints are behind the car (the highest crossed, plus one).
    pub fn checkpoints_reached(&self) -> u32 {
        self.next_checkpoint
    }

    /// True once the car has crossed the finish line.
    pub fn is_finished(&self) -> bool {
        self.finished
    }
}

impl Trigger {
    /// `point_m` in the marking's own frame.
    fn to_local(&self, point_m: [f32; 3]) -> Vector {
        self.pose.to_rapier().inverse() * Vector::from_array(point_m)
    }

    /// True if the local point is above or below the marking's footprint.
    fn is_over_footprint(&self, local: Vector) -> bool {
        local.x.abs() <= self.half_length_m && local.z.abs() <= self.half_width_m
    }

    /// True if the straight path `from` → `to` (local frame) touches the trigger box,
    /// which spans the footprint from `REACH_DOWN_M` below to `reach_up_m` above.
    /// The slab method: clip the path to each axis's range in turn.
    fn path_crosses(&self, from: Vector, to: Vector) -> bool {
        let low = [-self.half_length_m, -REACH_DOWN_M, -self.half_width_m];
        let high = [self.half_length_m, self.reach_up_m, self.half_width_m];
        let start = from.to_array();
        let travel = (to - from).to_array();
        let mut enter_t = 0.0_f32;
        let mut leave_t = 1.0_f32;
        for axis in 0..3 {
            if travel[axis] == 0.0 {
                if start[axis] < low[axis] || start[axis] > high[axis] {
                    return false;
                }
                continue;
            }
            let t_low = (low[axis] - start[axis]) / travel[axis];
            let t_high = (high[axis] - start[axis]) / travel[axis];
            enter_t = enter_t.max(t_low.min(t_high));
            leave_t = leave_t.min(t_low.max(t_high));
            if enter_t > leave_t {
                return false;
            }
        }
        true
    }
}

/// The action and reach of a marking piece, or None for pieces that are not triggers.
/// `checkpoint_number` is the number the next checkpoint gets.
fn trigger_action(
    piece_index: usize,
    piece: &PieceSpec,
    checkpoint_number: u32,
) -> Option<(TriggerAction, f32)> {
    match piece {
        PieceSpec::Checkpoint { pos, yaw } => {
            let action = TriggerAction::Checkpoint {
                number: checkpoint_number,
                respawn_position_m: [pos[0], pos[1] + RESPAWN_HEIGHT_M, pos[2]],
                yaw_rad: yaw.to_radians(),
            };
            Some((action, LINE_REACH_UP_M))
        }
        PieceSpec::Finish { .. } => Some((TriggerAction::Finish, LINE_REACH_UP_M)),
        PieceSpec::KickPlate {
            impulse, angular, ..
        } => {
            let action = TriggerAction::KickPlate {
                // Courses have at most 200 pieces (`course::MAX_PIECES`), so this fits.
                piece: piece_index as u32,
                impulse_n_s: *impulse,
                angular_n_m_s: *angular,
            };
            Some((action, KICK_PLATE_REACH_UP_M))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    // Test setup may panic; that is a test failure, not a library bug.
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::course_build::build_course_shapes;

    fn finish_line_at_40_m() -> CourseTriggers {
        let text = r#"{"version": 1, "name": "T", "start": {"pos": [0, 0.7, 0]},
            "bounds": {"min": [-10, -20, -20], "max": [100, 50, 20]},
            "pieces": [{"type": "finish", "pos": [40, 0, 0]}]}"#;
        let course = CourseSpec::from_json_str(text).unwrap();
        CourseTriggers::from_course(&course, &build_course_shapes(&course))
    }

    #[test]
    fn a_step_that_jumps_clean_over_a_line_still_crosses_it() {
        let mut triggers = finish_line_at_40_m();
        triggers.update([38.0, 0.6, 0.0]);

        // Both points are outside the 0.5 m line; only the path between them crosses.
        let hits = triggers.update([42.0, 0.6, 0.0]);

        assert_eq!(hits, vec![TriggerAction::Finish]);
    }

    #[test]
    fn a_step_that_passes_beside_a_line_does_not_cross_it() {
        let mut triggers = finish_line_at_40_m();
        triggers.update([38.0, 0.6, 12.0]);

        // The line is 20 m wide, so z = 12 m is beside its end.
        let hits = triggers.update([42.0, 0.6, 12.0]);

        assert!(hits.is_empty());
    }
}
