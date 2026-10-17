//! The `ninelives_sim` Python module.
//!
//! M5 adds `BatchedSim` (SPEC §9.1). For now the module exposes the fixed
//! conventions, so Python tests can check they import the right build.

use nl_sim::conventions;
use pyo3::prelude::*;

/// The nozzle names in their fixed index order.
#[pyfunction]
fn nozzle_names() -> Vec<&'static str> {
    conventions::NOZZLE_NAMES.to_vec()
}

/// The physics step length, in seconds.
#[pyfunction]
fn physics_dt_s() -> f32 {
    conventions::PHYSICS_DT_S
}

/// The Python module definition. The function name must match the module name.
#[pymodule]
fn ninelives_sim(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(nozzle_names, module)?)?;
    module.add_function(wrap_pyfunction!(physics_dt_s, module)?)?;
    Ok(())
}
