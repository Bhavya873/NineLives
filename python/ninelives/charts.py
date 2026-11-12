"""Draws the README charts (SVG, no plotting library) from the committed results.

    python -m ninelives.charts --repo . --out docs/images

Writes `results-by-stage.svg` (success rate per stage, Off / PD / Hybrid, both modes, from
`results/eval_*.json`) and `viewer-curve.svg` (the training viewer's checkpoint curve, from
`curve.json` of the newest run in `web/public/checkpoints/`).
"""

import argparse
import json
import logging
from pathlib import Path

STAGES = "ABCDE"
CONTROLLERS = [
    ("off", "Off", "#9aa5b1"),
    ("pd", "PD", "#2e86ab"),
    ("hybrid", "Hybrid (PD + AI)", "#e4572e"),
]
MODES = ["realistic", "arcade"]

log = logging.getLogger(__name__)


def success_rate(results: Path, controller: str, stage: str, mode: str) -> float | None:
    """Success rate of one eval file, or None when that controller was not run there."""
    path = results / f"eval_{controller}_{stage}_{mode}.json"
    if not path.is_file():
        return None
    return float(json.loads(path.read_text())["summary"]["success_rate"])


BAR_PANEL_WIDTH = 360
BAR_PANEL_HEIGHT = 260
BAR_TOP = 40
BAR_BOTTOM = 60
BAR_LEFT = 40


def bar_panel_parts(results: Path, mode: str, mode_index: int) -> list[str]:
    """The SVG elements of one mode's panel: title, gridlines and one bar group per stage."""
    panel_w, top, left = BAR_PANEL_WIDTH, BAR_TOP, BAR_LEFT
    plot_h = BAR_PANEL_HEIGHT - top - BAR_BOTTOM
    x0 = left + mode_index * (panel_w + 10)
    parts = [
        f'<text x="{x0}" y="22" font-weight="600">{mode.capitalize()}, 1000 episodes per bar</text>'
    ]
    for tick in (0, 0.5, 1.0):
        y = top + plot_h * (1 - tick)
        parts.append(
            f'<line x1="{x0}" x2="{x0 + panel_w - 20}" y1="{y:.1f}" y2="{y:.1f}" stroke="#ddd"/>'
        )
        if mode_index == 0:
            parts.append(
                f'<text x="{x0 - 6}" y="{y + 4:.1f}" text-anchor="end" fill="#555">{int(tick * 100)}%</text>'
            )
    group_w = (panel_w - 20) / len(STAGES)
    for stage_index, stage in enumerate(STAGES):
        parts.extend(bar_group_parts(results, stage, mode, x0 + stage_index * group_w, group_w))
    return parts


def bar_group_parts(
    results: Path, stage: str, mode: str, group_x: float, group_w: float
) -> list[str]:
    """The stage label and one bar per controller; a controller with no eval file gets no bar."""
    plot_h = BAR_PANEL_HEIGHT - BAR_TOP - BAR_BOTTOM
    parts = [
        f'<text x="{group_x + group_w / 2:.1f}" y="{BAR_TOP + plot_h + 16}" text-anchor="middle">Stage {stage}</text>'
    ]
    bar_w = (group_w - 8) / len(CONTROLLERS)
    for bar_index, (key, _, color) in enumerate(CONTROLLERS):
        rate = success_rate(results, key, stage, mode)
        if rate is None:
            continue
        h = plot_h * rate
        parts.append(
            f'<rect x="{group_x + 4 + bar_index * bar_w:.1f}" y="{BAR_TOP + plot_h - h:.1f}" '
            f'width="{bar_w - 1:.1f}" height="{h:.1f}" fill="{color}"/>'
        )
    return parts


def legend_parts(legend_y: float) -> list[str]:
    """One colour swatch and name per controller, in a row at `legend_y`."""
    parts = []
    for index, (_, name, color) in enumerate(CONTROLLERS):
        lx = BAR_LEFT + index * 160
        parts.append(f'<rect x="{lx}" y="{legend_y - 9}" width="10" height="10" fill="{color}"/>')
        parts.append(f'<text x="{lx + 14}" y="{legend_y}">{name}</text>')
    return parts


def by_stage_svg(results: Path) -> str:
    """Grouped bars: one panel per mode, one group per stage, one bar per controller."""
    width = 2 * BAR_PANEL_WIDTH + BAR_LEFT + 20
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {BAR_PANEL_HEIGHT + 40}" '
        'font-family="system-ui, sans-serif" font-size="11">',
        '<rect width="100%" height="100%" fill="#ffffff"/>',
    ]
    for mode_index, mode in enumerate(MODES):
        parts.extend(bar_panel_parts(results, mode, mode_index))
    parts.extend(legend_parts(BAR_PANEL_HEIGHT + 20))
    parts.append("</svg>")
    return "\n".join(parts)


CURVE_PALETTE = ["#2e86ab", "#76b041", "#e4572e", "#f6ae2d", "#6f4e7c", "#555555"]
CURVE_WIDTH = 560
CURVE_HEIGHT = 280
CURVE_LEFT = 44
CURVE_TOP = 36
CURVE_BOTTOM = 44


def curve_stage_parts(curve: dict, stage: str, index: int, max_steps: int) -> list[str]:
    """One stage's success-rate line over the checkpoints, plus its label under the plot."""
    plot_w = CURVE_WIDTH - CURVE_LEFT - 20
    plot_h = CURVE_HEIGHT - CURVE_TOP - CURVE_BOTTOM
    points = [
        (
            CURVE_LEFT + plot_w * entry["steps"] / max_steps,
            CURVE_TOP + plot_h * (1 - entry["stages"][stage]["success_rate"]),
        )
        for entry in curve["checkpoints"]
        if stage in entry["stages"]
    ]
    text = " ".join(f"{x:.1f},{y:.1f}" for x, y in points)
    color = CURVE_PALETTE[index % len(CURVE_PALETTE)]
    return [
        f'<polyline points="{text}" fill="none" stroke="{color}" stroke-width="1.8"/>',
        f'<text x="{CURVE_LEFT + index * 60}" y="{CURVE_HEIGHT - 10}" fill="{color}">Stage {stage}</text>',
    ]


def curve_svg(curve_json: Path) -> str:
    """Success rate against training steps for each stage, from the viewer's curve file."""
    curve = json.loads(curve_json.read_text())
    left, top = CURVE_LEFT, CURVE_TOP
    plot_w = CURVE_WIDTH - left - 20
    plot_h = CURVE_HEIGHT - top - CURVE_BOTTOM
    max_steps = max(entry["steps"] for entry in curve["checkpoints"])
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {CURVE_WIDTH} {CURVE_HEIGHT}" '
        'font-family="system-ui, sans-serif" font-size="11">',
        '<rect width="100%" height="100%" fill="#ffffff"/>',
        f'<text x="{left}" y="20" font-weight="600">{curve["title"]} ({curve["episodes"]} episodes per point)</text>',
    ]
    for tick in (0, 0.5, 1.0):
        y = top + plot_h * (1 - tick)
        parts.append(
            f'<line x1="{left}" x2="{left + plot_w}" y1="{y:.1f}" y2="{y:.1f}" stroke="#ddd"/>'
        )
        parts.append(
            f'<text x="{left - 6}" y="{y + 4:.1f}" text-anchor="end" fill="#555">{int(tick * 100)}%</text>'
        )
    for index, stage in enumerate(curve["stages"]):
        parts.extend(curve_stage_parts(curve, stage, index, max_steps))
    parts.append(
        f'<text x="{left + plot_w}" y="{CURVE_HEIGHT - 26}" text-anchor="end" fill="#555">'
        f"training steps (max {max_steps / 1e6:.0f}M)</text>"
    )
    parts.append("</svg>")
    return "\n".join(parts)


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--repo", type=Path, default=Path("."), help="repo root")
    parser.add_argument("--out", type=Path, default=Path("docs/images"), help="folder for the SVGs")
    args = parser.parse_args(argv)
    # progress messages go to the console
    logging.basicConfig(level=logging.INFO, format="%(message)s")
    args.out.mkdir(parents=True, exist_ok=True)
    # chart 1: success rate per stage
    by_stage_path = args.out / "results-by-stage.svg"
    by_stage_path.write_text(by_stage_svg(args.repo / "results") + "\n")
    log.info(f"wrote {by_stage_path}")
    # chart 2: the viewer's training curve
    checkpoints = args.repo / "web" / "public" / "checkpoints"
    newest_run = json.loads((checkpoints / "index.json").read_text())["runs"][-1]["run_id"]
    curve = checkpoints / newest_run / "curve.json"
    curve_path = args.out / "viewer-curve.svg"
    curve_path.write_text(curve_svg(curve) + "\n")
    log.info(f"wrote {curve_path}")


if __name__ == "__main__":
    main()
