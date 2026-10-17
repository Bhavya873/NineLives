"""Checks that the Rust module `ninelives_sim` is built and importable."""

import ninelives
import ninelives_sim


def test_package_has_a_version():
    assert ninelives.__version__ == "0.1.0"


def test_sim_module_reports_ten_nozzles_in_spec_order():
    names = ninelives_sim.nozzle_names()

    assert len(names) == 10
    assert names[0] == "L_FL"
    assert names[9] == "B_R"


def test_sim_module_runs_physics_at_120_hz():
    assert abs(1.0 / ninelives_sim.physics_dt_s() - 120.0) < 1e-3
