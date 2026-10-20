"""PD tuning tests (M7 round 6): the gains are read from and written back into
controllers.toml without touching other sections, scaling never flips a sign, and a tiny
CMA-ES run writes a controllers.toml PD can load."""

import math

from ninelives.sim_config import CONFIGS_DIR
from ninelives.tune_pd import main, pd_gains, scaled_gains, with_pd_gains

REPO_TOML = (CONFIGS_DIR / "controllers.toml").read_text()


def test_new_gains_replace_only_the_pd_numbers():
    gains = pd_gains(REPO_TOML)
    gains["kp"] = 9.5

    text = with_pd_gains(REPO_TOML, gains)

    assert pd_gains(text)["kp"] == 9.5
    assert pd_gains(text)["kd"] == pd_gains(REPO_TOML)["kd"]
    assert text.split("[allocation]")[1] == REPO_TOML.split("[allocation]")[1]


def test_scaling_multiplies_each_gain_and_caps_the_upright_threshold():
    base = pd_gains(REPO_TOML)
    log_factors = [0.0] * len(base)
    log_factors[0] = math.log(2.0)
    log_factors[list(base).index("upright_alignment")] = 1.0

    gains = scaled_gains(base, log_factors)

    assert math.isclose(gains["kp"], 2.0 * base["kp"])
    assert gains["upright_alignment"] == 0.99
    assert all(value > 0.0 for value in gains.values())


def test_a_tiny_tuning_run_writes_a_loadable_controllers_toml(tmp_path):
    out = tmp_path / "controllers.toml"

    main(["--generations", "1", "--popsize", "2", "--episodes", "2", "--out", str(out)])

    assert set(pd_gains(out.read_text())) == set(pd_gains(REPO_TOML))
