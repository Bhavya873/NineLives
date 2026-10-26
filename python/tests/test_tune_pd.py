"""PD tuning tests (M7 round 6): the gains are read from and written back into
controllers.toml without touching other sections, scaling never flips a sign, and a tiny
CMA-ES run writes a controllers.toml PD can load."""

import math

import pytest

from ninelives.sim_config import CONFIGS_DIR
from ninelives.tune_pd import (
    BUMPS_AIR_LIMIT_KG,
    FINAL_TEST_FIRST_SEED,
    bumps_air_kg,
    main,
    pd_gains,
    scaled_gains,
    tunables,
    tuning_seeds_start,
    with_pd_gains,
    with_tunables,
)

REPO_TOML = (CONFIGS_DIR / "controllers.toml").read_text()


def test_tuning_seeds_never_reach_the_final_test_block():
    # 1000 generations of 1000 episodes end exactly at the block's first seed.
    last_allowed = tuning_seeds_start(999, 1000)

    assert last_allowed + 1000 <= FINAL_TEST_FIRST_SEED
    with pytest.raises(ValueError, match="final test block"):
        tuning_seeds_start(1000, 1000)


def test_new_gains_replace_only_the_pd_numbers():
    gains = pd_gains(REPO_TOML)
    gains["kp"] = 9.5

    text = with_pd_gains(REPO_TOML, gains)

    assert pd_gains(text)["kp"] == 9.5
    assert pd_gains(text)["kd"] == pd_gains(REPO_TOML)["kd"]
    assert text.split("[allocation]")[1] == REPO_TOML.split("[allocation]")[1]


def test_scaling_multiplies_each_gain_and_caps_a_fraction_below_1():
    base = pd_gains(REPO_TOML)
    log_factors = [0.0] * len(base)
    log_factors[0] = math.log(2.0)
    log_factors[list(base).index("ground_side_push")] = 2.0

    gains = scaled_gains(base, log_factors)

    assert math.isclose(gains["kp"], 2.0 * base["kp"])
    assert gains["ground_side_push"] == 0.99
    assert all(value > 0.0 for value in gains.values())


def test_a_tiny_tuning_run_writes_a_loadable_controllers_toml(tmp_path):
    out = tmp_path / "controllers.toml"

    main(["--generations", "1", "--popsize", "2", "--episodes", "2", "--out", str(out)])

    assert set(pd_gains(out.read_text())) == set(pd_gains(REPO_TOML))


def test_feel_the_comfort_cap_and_when_pd_acts_are_never_tuned():
    gains = pd_gains((CONFIGS_DIR / "controllers.toml").read_text())

    for name in ("air_pitch_rad", "max_angular_accel_rad_s2", "brake_speed_m_s"):
        assert name not in gains
    assert "kp" in gains


def test_the_repo_pd_stays_under_the_bumps_limit_so_it_pays_nothing():
    repo_toml = (CONFIGS_DIR / "controllers.toml").read_text()

    assert bumps_air_kg(repo_toml) < BUMPS_AIR_LIMIT_KG


def test_a_tuning_run_too_long_for_the_seed_range_is_refused_before_it_starts():
    from ninelives.tune_pd import tune

    # Refused at once: no generation runs (each would take minutes).
    with pytest.raises(ValueError, match="final test block"):
        tune(generations=1001, popsize=2, episodes=1000, sigma=0.1)


def test_the_lag_compensation_is_tuned_beside_pd_and_written_into_allocation():
    values = tunables(REPO_TOML)
    values["lag_compensation"] = 0.75
    values["kp"] = 8.0

    text = with_tunables(REPO_TOML, values)

    assert tunables(text)["lag_compensation"] == 0.75
    assert pd_gains(text)["kp"] == 8.0
    assert "lag_compensation" not in pd_gains(text)


def test_scaling_keeps_the_tumble_blend_band_open_and_in_order():
    base = tunables(REPO_TOML)
    log_factors = [0.0] * len(base)
    # Start pushed past end: 30° × e ≈ 82° against 60°.
    log_factors[list(base).index("tumble_blend_start_deg")] = 1.0

    gains = scaled_gains(base, log_factors)

    assert gains["tumble_blend_end_deg"] - gains["tumble_blend_start_deg"] >= 5.0


def test_when_pd_counts_the_car_as_lying_on_the_ground_is_never_tuned():
    assert "on_ground_height_m" not in pd_gains(REPO_TOML)
