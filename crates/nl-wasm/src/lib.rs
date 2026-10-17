//! The browser's view of the simulation.
//!
//! M2 adds the `Game` class (SPEC §11.1). For now the module exposes a short
//! build description so the web app can show that the wasm module loaded.

use nl_sim::conventions;
use wasm_bindgen::prelude::*;

/// A one-line description of this build, shown on the placeholder page.
#[wasm_bindgen]
pub fn build_info() -> String {
    format!(
        "nl-wasm {} · physics {} Hz · {} nozzles",
        env!("CARGO_PKG_VERSION"),
        (1.0 / conventions::PHYSICS_DT_S).round(),
        conventions::NOZZLE_COUNT
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_info_mentions_the_physics_rate() {
        assert!(build_info().contains("120 Hz"));
    }
}
