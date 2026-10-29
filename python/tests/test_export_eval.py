"""Export and eval tests (SPEC §9.12, §10, §12): the Rust forward pass matches SB3 on
1000 observations (< 1e-4), the Python eval reproduces the Rust eval runner's rows, and
an exported policy can be evaluated as the AI controller."""

import json

import numpy as np
import torch
from stable_baselines3 import PPO
from stable_baselines3.common.vec_env import VecNormalize

from ninelives.env import NineLivesVecEnv
from ninelives.eval import RESULTS_DIR, evaluate, write_report
from ninelives.eval import main as eval_main
from ninelives.export import evenly_spaced, export_policy
from ninelives.export import main as export_main
from ninelives.provenance import provenance
from ninelives.run_files import DEFAULT_POLICY_PATH, save_checkpoint
from ninelives.sim_config import load_sim_config
from ninelives_sim import obs_schema_hash, obs_spec, policy_actions, text_hash

OBS_DIM = len(obs_spec())

PARITY_TOLERANCE = 1e-4


def random_policy() -> tuple[PPO, VecNormalize]:
    """An untrained SB3 policy with large random weights and made-up normalizer stats,
    so the parity check exercises every layer and the normalization."""
    rng = np.random.default_rng(0)
    torch.manual_seed(0)
    normalizer = VecNormalize(NineLivesVecEnv(n_envs=2), clip_obs=10.0)
    model = PPO(
        "MlpPolicy",
        normalizer,
        n_steps=64,
        policy_kwargs={
            "net_arch": {"pi": [128, 128], "vf": [128, 128]},
            "activation_fn": torch.nn.Tanh,
        },
        device="cpu",
    )
    with torch.no_grad():
        for parameter in model.policy.parameters():
            parameter.normal_(0.0, 0.3)
    size = normalizer.observation_space.shape[0]
    normalizer.obs_rms.mean = rng.normal(0.0, 2.0, size)
    normalizer.obs_rms.var = rng.uniform(0.1, 5.0, size)
    return model, normalizer


def random_observations(count: int) -> np.ndarray:
    rng = np.random.default_rng(1)
    return rng.normal(0.0, 3.0, size=(count, OBS_DIM)).astype(np.float32)


def test_the_rust_forward_pass_matches_sb3_on_1000_observations():
    model, normalizer = random_policy()
    policy_json = json.dumps(export_policy(model, normalizer, "parity"))
    observations = random_observations(1000)

    rust_actions = policy_actions(policy_json, observations)
    sb3_actions, _ = model.predict(normalizer.normalize_obs(observations), deterministic=True)

    assert np.abs(rust_actions - sb3_actions).max() < PARITY_TOLERANCE
    assert np.abs(sb3_actions).max() > 0.5, "the test policy should not be near zero"


def test_the_export_carries_the_schema_hash_and_the_layer_shapes():
    model, normalizer = random_policy()

    policy = export_policy(model, normalizer, "shapes")

    assert policy["obs_schema_hash"] == obs_schema_hash()
    shapes = [(layer["in"], layer["out"], layer["act"]) for layer in policy["layers"]]
    assert shapes == [(OBS_DIM, 128, "tanh"), (128, 128, "tanh"), (128, 10, "none")]


def test_the_python_eval_reproduces_the_rust_eval_runners_rows():
    committed = json.loads((RESULTS_DIR / "eval_pd_A_realistic.json").read_text())

    report = evaluate("pd", "A", "realistic", episodes=20, first_seed=0)

    assert report["episodes"] == committed["episodes"][:20]


def test_an_exported_policy_runs_as_the_ai_controller(tmp_path):
    model, normalizer = random_policy()
    policy_path = tmp_path / "policy.json"
    policy_path.write_text(json.dumps(export_policy(model, normalizer, "ai")))

    report = evaluate("ai", "A", "arcade", episodes=8, first_seed=0, policy=policy_path)
    json_path = write_report(report, tmp_path)

    assert report["summary"]["episodes"] == 8
    assert json_path.with_suffix(".csv").read_text().count("\n") == 9


def test_viewer_checkpoints_are_spread_evenly_and_keep_first_and_last():
    folders = list(range(20))

    picks = evenly_spaced(folders, 8)

    assert len(picks) == 8
    assert picks[0] == 0 and picks[-1] == 19
    assert evenly_spaced(folders[:5], 8) == folders[:5]


def test_the_export_command_writes_a_policy_rust_can_load(tmp_path):
    model, normalizer = random_policy()
    run_dir = tmp_path / "run"
    save_checkpoint(run_dir / "latest", model, normalizer)
    out = tmp_path / "policy.json"

    export_main(["--run", str(run_dir), "--checkpoint", "latest", "--out", str(out)])

    observations = random_observations(5)
    rust_actions = policy_actions(out.read_text(), observations)
    sb3_actions, _ = model.predict(normalizer.normalize_obs(observations), deterministic=True)
    assert np.abs(rust_actions - sb3_actions).max() < PARITY_TOLERANCE


def test_eval_with_domain_randomization_changes_the_episodes_and_the_file_name(tmp_path):
    plain = evaluate("pd", "A", "realistic", episodes=10, first_seed=0)
    randomized = evaluate("pd", "A", "realistic", episodes=10, first_seed=0, randomize=True)

    assert plain["episodes"] != randomized["episodes"]
    assert write_report(randomized, tmp_path).name == "eval_pd_A_realistic_dr.json"


def test_an_eval_report_carries_the_hashes_that_tie_it_to_its_settings():
    report = evaluate("pd", "A", "realistic", episodes=2, first_seed=0)

    assert len(report["physics_hash"]) == 16
    assert len(report["controller_config_hash"]) == 16
    assert report["policy_hash"] is None
    assert report["git_sha"]


def test_a_hybrid_report_hashes_the_policy_files_text():
    report = evaluate("hybrid", "A", "realistic", episodes=1, first_seed=0)

    assert report["policy_hash"] == text_hash([DEFAULT_POLICY_PATH.read_text()])


def test_a_changed_setting_changes_only_the_hash_it_belongs_to():
    sim_config = load_sim_config("realistic", "A")
    base = provenance(sim_config, None)

    physics = json.loads(sim_config)
    physics["vehicle_toml"] += "\n# edited"
    controllers = json.loads(sim_config)
    controllers["controllers_toml"] += "\n# edited"
    after_physics = provenance(json.dumps(physics), None)
    after_controllers = provenance(json.dumps(controllers), None)

    assert after_physics["physics_hash"] != base["physics_hash"]
    assert after_physics["controller_config_hash"] == base["controller_config_hash"]
    assert after_controllers["controller_config_hash"] != base["controller_config_hash"]
    assert after_controllers["physics_hash"] == base["physics_hash"]


def test_the_eval_command_writes_into_the_folder_given_by_out(tmp_path):
    eval_main(["--controller", "pd", "--episodes", "2", "--out", str(tmp_path)])

    written = json.loads((tmp_path / "eval_pd_A_realistic.json").read_text())
    assert written["physics_hash"] and written["summary"]["episodes"] == 2


def test_viewer_checkpoints_end_on_the_shipped_policy(tmp_path, monkeypatch):
    import ninelives.export as export

    model, normalizer = random_policy()
    run_dir = tmp_path / "run"
    for steps in (100, 200, 300):
        model.num_timesteps = steps
        save_checkpoint(run_dir / "checkpoints" / str(steps), model, normalizer)
    model.num_timesteps = 200
    save_checkpoint(run_dir / "best", model, normalizer)
    monkeypatch.setattr(export, "VIEWER_DIR", tmp_path / "viewer")

    written = export.export_viewer_checkpoints(run_dir, run_dir / "best")

    steps = [json.loads(path.read_text())["trained"]["steps"] for path in written]
    assert steps == [100, 200]
    index = json.loads((tmp_path / "viewer" / "index.json").read_text())
    assert index["runs"][0]["run_id"] == "run"
    assert [entry["steps"] for entry in index["runs"][0]["checkpoints"]] == [100, 200]
