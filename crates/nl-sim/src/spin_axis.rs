//! What an episode started with, for the eval's per-preset and per-axis tables: the plate
//! or scripted entry's name, its strength and the axis the car mostly spins about.

use serde::Serialize;

/// The share of a spin's squared size one axis needs to be called the dominant one.
const DOMINANT_SHARE: f32 = 0.6;
/// Spin below this size (rad/s) or impulse below this size (N·m·s) has no axis.
const NO_AXIS_BELOW: f32 = 1e-3;

/// The axis a spin is mostly about, in the car's own frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SpinAxis {
    /// About the nose (+X): a barrel roll.
    Roll,
    /// About the side (+Z): a front or back flip.
    Pitch,
    /// About the up axis (+Y): a flat spin.
    Yaw,
    /// No axis holds 60% of it.
    Mixed,
}

impl SpinAxis {
    /// The axis `[roll, yaw, pitch]` (the body-frame order, X, Y, Z) is mostly about, or
    /// None for a zero vector.
    pub fn dominant(body: [f32; 3]) -> Option<SpinAxis> {
        let [x, y, z] = body;
        let total = x * x + y * y + z * z;
        if total.sqrt() < NO_AXIS_BELOW {
            return None;
        }
        let share = |value: f32| value * value / total;
        if share(x) >= DOMINANT_SHARE {
            Some(SpinAxis::Roll)
        } else if share(z) >= DOMINANT_SHARE {
            Some(SpinAxis::Pitch)
        } else if share(y) >= DOMINANT_SHARE {
            Some(SpinAxis::Yaw)
        } else {
            Some(SpinAxis::Mixed)
        }
    }

    /// The name used in tables and JSON.
    pub fn name(self) -> &'static str {
        match self {
            SpinAxis::Roll => "roll",
            SpinAxis::Pitch => "pitch",
            SpinAxis::Yaw => "yaw",
            SpinAxis::Mixed => "mixed",
        }
    }
}

/// What an episode's start was made of. Empty for stages without a plate or scripted
/// entry (and for Stage A's plain random drops, which only have an axis).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScenarioTag {
    /// The preset's name ("barrel_left", "scripted_flip"), if there is one.
    pub preset: Option<String>,
    /// Stage F: the plate's strength multiplier. Stage A: the start spin in rad/s.
    pub strength: Option<f32>,
    /// The axis the plate or the start spin is mostly about.
    pub spin_axis: Option<SpinAxis>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_biggest_share_names_the_axis_and_an_even_split_is_mixed() {
        assert_eq!(SpinAxis::dominant([3.0, 0.0, 0.0]), Some(SpinAxis::Roll));
        assert_eq!(SpinAxis::dominant([0.0, 0.0, -2.0]), Some(SpinAxis::Pitch));
        assert_eq!(SpinAxis::dominant([0.5, 2.0, 0.5]), Some(SpinAxis::Yaw));
        assert_eq!(SpinAxis::dominant([2.0, 0.0, 2.0]), Some(SpinAxis::Mixed));
        assert_eq!(SpinAxis::dominant([0.0; 3]), None);
    }
}
