//! Fingerprints that tie an eval artifact to the exact settings and policy it ran on, so a
//! baseline recorded under one physics can never be mistaken for a result under another
//! (plan G1). Built on the FNV-1a of `hash`; the Rust eval runner and the Python eval both
//! call these, so one config gives one value in both.

use crate::hash::{FNV_OFFSET_BASIS, fnv1a_add_bytes};
use crate::modes::DEFAULT_MODES_TOML;
use crate::vehicle_config::DEFAULT_VEHICLE_TOML;

/// The repo's approach rules (docs/contracts/traversal.md), embedded like the other defaults.
pub const DEFAULT_APPROACH_TOML: &str = include_str!("../../../configs/approach.toml");

/// A short fingerprint (16 hex digits) of `parts`, in order. Each part is hashed with its
/// length first, so moving text from one part to the next changes the value.
pub fn text_hash(parts: &[&str]) -> String {
    let mut hash = FNV_OFFSET_BASIS;
    for part in parts {
        hash = fnv1a_add_bytes(hash, &(part.len() as u64).to_le_bytes());
        hash = fnv1a_add_bytes(hash, part.as_bytes());
    }
    format!("{hash:016x}")
}

/// The fingerprint of everything that decides how the car and the world behave: the vehicle,
/// the modes, the piece rules and the approach rules, as their file texts. Controller
/// settings and the policy are fingerprinted separately.
pub fn physics_hash(
    vehicle_toml: &str,
    modes_toml: &str,
    pieces_json: &str,
    approach_toml: &str,
) -> String {
    text_hash(&[vehicle_toml, modes_toml, pieces_json, approach_toml])
}

/// `physics_hash` of the repo's own settings, the ones this build embeds.
pub fn default_physics_hash() -> String {
    physics_hash(
        DEFAULT_VEHICLE_TOML,
        DEFAULT_MODES_TOML,
        crate::course_gen::DEFAULT_PIECES_JSON,
        DEFAULT_APPROACH_TOML,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_16_hex_digits_and_the_same_every_time() {
        let hash = text_hash(&["a", "b"]);

        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(hash, text_hash(&["a", "b"]));
    }

    #[test]
    fn the_hash_depends_on_the_order_of_the_parts() {
        assert_ne!(text_hash(&["a", "b"]), text_hash(&["b", "a"]));
    }

    #[test]
    fn text_moved_from_one_part_to_the_next_changes_the_hash() {
        assert_ne!(text_hash(&["ab", "c"]), text_hash(&["a", "bc"]));
    }

    #[test]
    fn each_physics_input_changes_the_physics_hash() {
        let base = physics_hash("v", "m", "p", "a");

        assert_ne!(base, physics_hash("v2", "m", "p", "a"));
        assert_ne!(base, physics_hash("v", "m2", "p", "a"));
        assert_ne!(base, physics_hash("v", "m", "p2", "a"));
        assert_ne!(base, physics_hash("v", "m", "p", "a2"));
    }

    #[test]
    fn the_default_physics_hash_covers_the_embedded_settings() {
        assert_eq!(
            default_physics_hash(),
            physics_hash(
                DEFAULT_VEHICLE_TOML,
                DEFAULT_MODES_TOML,
                crate::course_gen::DEFAULT_PIECES_JSON,
                DEFAULT_APPROACH_TOML
            )
        );
    }
}
