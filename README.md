# Nine Lives

A browser driving game where a car fires 10 cold-gas air nozzles to flip itself upright in mid-air and land on its wheels.

> Inspired by the thruster-assisted Roadster demo of October 2026. This is a generic low-poly car and an independent project: no affiliation with Tesla or SpaceX, and no logos or names from either. The numbers are plausible, not official.

![The game just after a drop test: key legend, air gauge, g-meter and the peak of the landing](docs/images/game.png)

![The training viewer: eight checkpoints of the AI, 8 cars each](docs/images/viewer.png)

## Play

Open the live demo: TODO (the owner adds the link once the game is hosted).

| Key | Action |
|---|---|
| Arrow keys | drive, brake, steer |
| Space | hop |
| Shift | boost |
| A / S | barrel roll left / right |
| D / F | front flip / back flip |
| X | drop test: throw the car into a random bad start |
| M | switch between Limited air and Unlimited air |
| V | change the wind |
| Esc | pause |

R puts the car back on the road. A stunt key is a request, not a command: the car only starts the move if it can still finish it and land.

## Limited air and Unlimited air

The car carries an air tank of 260 kg (the `air_left_kg` field in `results/eval_off_A_realistic.json`). Every nozzle burst uses some of it.

- **Limited air:** the tank drains while you fire. It refills slowly once the wheels are down and the nozzles are quiet. If it runs dry, the car cannot right itself. This is the default.
- **Unlimited air:** the tank never empties and the nozzles push twice as hard. Use it to play with the controls and stunts without watching the gauge. Air use is still counted.

In code, result files and settings, Limited air is called `realistic` and Unlimited air is called `arcade`. Those names stay in filenames such as `results/eval_pd_A_realistic.json`.

## How it works

- **The car.** A simulated car with four wheels and 10 air nozzles. The physics is written in Rust and runs in your browser as WebAssembly. It runs at a fixed 120 steps per second, so the same inputs always give the same result.
- **The safety net.** A plain controller (PD, which means it steers the car back toward upright in proportion to how far off it is and how fast it is turning) decides which nozzles to fire. It is the default flyer in the game.
- **The trained AI.** A small neural network trained in simulation with reinforcement learning (it tries drops, gets a reward for clean landings and learns from that). When the car tips more than 120 degrees from upright, the AI takes over from PD. The game only uses it if you open the page with `?controller=hybrid`. The AI is only trusted up to 55 m/s; above that PD flies.
- **Training stages.** Stage A is a drop from a random bad start. Stages B to E add ramps, walls, gaps and obstacle courses. Stage F mixes everything.

## What we found

All numbers are success rates over 1000 fixed episodes per cell (four wheels down and quiet for a second), read from `results/eval_<controller>_<stage>_<mode>.json`, field `summary.success_rate`.

- With no controller, 20.6% of Stage A drops end on four wheels (`eval_off_A_realistic.json`). PD raises that to 82.1% in Limited air (`eval_pd_A_realistic.json`) and 87.6% in Unlimited air (`eval_pd_A_arcade.json`).
- The trained AI does not beat PD everywhere. On Stage A with Limited air, PD plus the AI gets 80.8% against PD's 82.1% (`eval_hybrid_A_realistic.json`). It uses less air per save though: 43 kg against 49 kg (`summary.air_per_save_kg` in the same two files).
- Stages B, C and D are level between the two: 99.6%, 100.0% and 99.8% (`eval_pd_B_realistic.json`, `eval_pd_C_realistic.json`, `eval_pd_D_realistic.json`, and the matching `eval_hybrid_*` files).
- Stage E, the hardest course, is 91.0% for PD and 91.3% for PD plus the AI in Limited air (`eval_pd_E_realistic.json`, `eval_hybrid_E_realistic.json`).
- **The hover hack.** We trained the same car twice for about 12 million steps on Stage A in Unlimited air. One reward paid +0.1 for every step spent upright. The other paid only for landing and charged for air. With the first reward, 37.0% of drops were still in the air when the 8 second clock ran out. With the second, none were. Mean air used was 135 kg against 35 kg (`results/hover_hack.json`, `broken` and `fixed`). This is one seed and measures time-outs and airtime, not the car's attitude, so read it as a demonstration, not a benchmark.
- TODO: the retrain (round r5) results are not in `results/` yet. Add the new per-preset landing table and the Stage A to F numbers from the r5 evaluation here, and refresh the numbers above if the eval files change.

The full tables and every run are in [docs/EXPERIMENTS.md](docs/EXPERIMENTS.md). The file index is [results/README.md](results/README.md).

![Success rate by stage and mode: Off, PD, Hybrid](docs/images/results-by-stage.svg)

## How it was built

The work followed a milestone plan ([docs/BUILD-PLAN.md](docs/BUILD-PLAN.md)). Each experiment and what it taught us is in [docs/EXPERIMENTS.md](docs/EXPERIMENTS.md), and each design choice with its reason is in [docs/DECISIONS.md](docs/DECISIONS.md). The AI is explained without jargon in [docs/AI-IN-PLAIN-WORDS.md](docs/AI-IN-PLAIN-WORDS.md).

- **Same physics everywhere.** One Rust crate holds the physics, and both training and the browser use it. A frozen test run (the golden trace) must give the same result on native and WebAssembly builds, and each physics setup has its own hash.
- **Checks on every change.** Continuous integration runs the Rust, Python and browser tests.
- **Numbers have a source.** Every number in this README comes from a file under `results/`, and the files are only overwritten by a rerun.

## Honest limits

- PD alone is already strong, and the trained AI is not better on every stage.
- Frame rate on a real GPU has not been measured. The 60 fps figures come from software rendering in headless Chromium.
- The training viewer cannot show Stages C and E, because each seed builds its own course.
- Stage A is the hard one: in Limited air, 17.9% of drops still crash with PD (`crash_rate` in `results/eval_pd_A_realistic.json`).
- The hover-hack runs are small: 12 million steps, one seed each, 200 drops.
- The AI is trusted only up to 55 m/s.

## Credits

- Inspired by the thruster-assisted Roadster demo of 2026-10-15: [TechCrunch](https://techcrunch.com/2026/10/15/tesla-reveals-spacex-powered-roadster-that-flies-and-falls-with-style/), [Glitchwire](https://glitchwire.com/news/the-tesla-roadster-hovered-flipped-and-landed-the-air-tank-weighed-260-kilos/).
- Fonts: [Barlow and Barlow Condensed](https://github.com/jpt/barlow), SIL Open Font License 1.1.
- Built with [Rapier](https://rapier.rs/) (physics), [three.js](https://threejs.org/) (graphics) and [Stable-Baselines3](https://stable-baselines3.readthedocs.io/) (PPO training).
