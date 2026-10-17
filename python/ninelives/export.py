"""Exports a trained policy to the JSON the Rust runtime and the browser load (SPEC §9.12).

    python -m ninelives.export --run runs/<name> --out models/policy.json
    python -m ninelives.export --run runs/<name> --viewer   # also the training viewer files

The file holds the VecNormalize statistics and the MLP layers (SB3's
`mlp_extractor.policy_net` plus `action_net`), row-major `out × in`. Viewer checkpoints
are up to 8 evenly spaced numbered checkpoints, rounded to 4 significant digits, written
to `web/public/checkpoints/<run-id>/<n>.json` (under 3 MB together).
"""

import argparse
import json
import subprocess
from pathlib import Path
from typing import Any

from stable_baselines3 import PPO
from stable_baselines3.common.vec_env import VecNormalize
from torch import nn

from ninelives.run_files import DEFAULT_POLICY_PATH, load_model_and_normalizer
from ninelives.sim_config import REPO_ROOT
from ninelives_sim import obs_schema_hash, policy_format

VIEWER_DIR = REPO_ROOT / "web" / "public" / "checkpoints"
VIEWER_MAX_CHECKPOINTS = 8
VIEWER_MAX_BYTES = 3_000_000
VIEWER_SIGNIFICANT_DIGITS = 4


def export_policy(
    model: PPO, normalizer: VecNormalize, run_id: str, digits: int | None = None
) -> dict[str, Any]:
    """The policy file's contents; `digits` rounds every number to that many
    significant digits (None keeps full float32 precision)."""
    stats = normalizer.obs_rms
    file_format, version = policy_format()
    return {
        "format": file_format,
        "version": version,
        "obs_schema_hash": obs_schema_hash(),
        "obs_dim": int(stats.mean.shape[0]),
        "act_dim": int(model.action_space.shape[0]),
        "obs_norm": {
            "mean": rounded(stats.mean.tolist(), digits),
            "var": rounded(stats.var.tolist(), digits),
            "eps": float(normalizer.epsilon),
            "clip": float(normalizer.clip_obs),
        },
        "layers": policy_layers(model, digits),
        "action": {"clip": [-1, 1], "map": "(a+1)/2"},
        "trained": {"run_id": run_id, "steps": int(model.num_timesteps), "git_sha": git_sha()},
    }


def policy_layers(model: PPO, digits: int | None) -> list[dict[str, Any]]:
    """The actor's dense layers in order, each with the activation that follows it."""
    modules = [*model.policy.mlp_extractor.policy_net, model.policy.action_net]
    layers = []
    for index, module in enumerate(modules):
        if not isinstance(module, nn.Linear):
            continue
        following = modules[index + 1] if index + 1 < len(modules) else None
        if following is not None and not isinstance(following, (nn.Tanh, nn.Linear)):
            raise ValueError(f"export supports tanh MLPs only, found {following}")
        layers.append(
            {
                "w": rounded(module.weight.detach().flatten().tolist(), digits),
                "b": rounded(module.bias.detach().tolist(), digits),
                "in": module.in_features,
                "out": module.out_features,
                "act": "tanh" if isinstance(following, nn.Tanh) else "none",
            }
        )
    return layers


def rounded(values: list[float], digits: int | None) -> list[float]:
    """`values` rounded to `digits` significant digits (unchanged when None)."""
    if digits is None:
        return values
    return [float(f"{value:.{digits}g}") for value in values]


def git_sha() -> str:
    """The commit the export was made from, with "-dirty" if configs or code differ from
    it (then the commit alone cannot reproduce the run); "unknown" outside a checkout."""
    try:
        head = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=REPO_ROOT, capture_output=True, text=True
        )
        changed = subprocess.run(
            ["git", "status", "--porcelain", "--", "configs", "crates", "python"],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
    except OSError:
        return "unknown"
    sha = head.stdout.strip() or "unknown"
    return f"{sha}-dirty" if changed.stdout.strip() else sha


def evenly_spaced(items: list[Path], count: int) -> list[Path]:
    """Up to `count` items spread evenly from first to last (both kept)."""
    if len(items) <= count:
        return items
    picks = {round(index * (len(items) - 1) / (count - 1)) for index in range(count)}
    return [items[index] for index in sorted(picks)]


def export_viewer_checkpoints(run_dir: Path, shipped: Path) -> list[Path]:
    """SPEC §9.12: the training viewer's checkpoint files for this run. They run up to
    the shipped policy (`shipped`, the folder exported to models/policy.json), which is
    always the last one, so the viewer ends on the policy the game plays."""
    shipped_steps = load_model_and_normalizer(shipped)[0].num_timesteps
    folders = [folder for folder in (run_dir / "checkpoints").iterdir() if folder.name.isdigit()]
    earlier = [folder for folder in folders if int(folder.name) < shipped_steps]
    earlier.sort(key=lambda folder: int(folder.name))
    numbered = [*evenly_spaced(earlier, VIEWER_MAX_CHECKPOINTS - 1), shipped]
    out_dir = VIEWER_DIR / run_dir.name
    out_dir.mkdir(parents=True, exist_ok=True)
    for old_file in out_dir.glob("*.json"):
        old_file.unlink()  # a re-export may write fewer files than the last one
    written = []
    for number, folder in enumerate(numbered, start=1):
        model, normalizer = load_model_and_normalizer(folder)
        policy = export_policy(model, normalizer, run_dir.name, VIEWER_SIGNIFICANT_DIGITS)
        path = out_dir / f"{number}.json"
        path.write_text(json.dumps(policy, separators=(",", ":")))
        written.append(path)
    total_bytes = sum(path.stat().st_size for path in written)
    if total_bytes > VIEWER_MAX_BYTES:
        raise ValueError(f"viewer checkpoints are {total_bytes} bytes, over {VIEWER_MAX_BYTES}")
    return written


def main(argv: list[str] | None = None) -> None:
    """The command line (see the module docstring)."""
    parser = argparse.ArgumentParser(description="Export a trained policy to JSON.")
    parser.add_argument("--run", type=Path, required=True, help="runs/<name>")
    parser.add_argument("--checkpoint", default="best", help="best, latest or a step count")
    parser.add_argument("--out", type=Path, default=DEFAULT_POLICY_PATH)
    parser.add_argument("--viewer", action="store_true", help="also write viewer checkpoints")
    args = parser.parse_args(argv)
    folder = args.run / args.checkpoint
    if args.checkpoint.isdigit():
        folder = args.run / "checkpoints" / args.checkpoint
    if not folder.is_dir():
        raise SystemExit(
            f"{folder} does not exist (best/ appears after the first 1M-step eval); "
            "try --checkpoint latest"
        )
    model, normalizer = load_model_and_normalizer(folder)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(export_policy(model, normalizer, args.run.name)))
    print(f"wrote {args.out} ({model.num_timesteps} steps)")
    if args.viewer:
        for path in export_viewer_checkpoints(args.run, folder):
            print(f"wrote {path}")


if __name__ == "__main__":
    main()
