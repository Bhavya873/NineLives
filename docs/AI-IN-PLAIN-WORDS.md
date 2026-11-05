# How the AI learns, in plain words

What we built for training (milestone 5), and how the pieces fit. Short version first, details after.

## The one-line version

We taught a small neural network to fire the car's 10 thrusters so a falling, tumbling car lands on its wheels. It learned by trying millions of drops in our own physics simulator.

## The big picture

```
   configs/*.toml          (all the numbers: car, thrusters, reward, start ranges)
        │
        ▼
 ┌──────────────────────────── Rust ────────────────────────────┐
 │  nl-sim: the physics world. Drops the car, steps time,       │
 │          decides "landed / crashed / out of time",           │
 │          builds what the AI sees (43 numbers),               │
 │          scores each moment (the reward).                    │
 │  nl-py:  lets Python drive 64 of these worlds at once.       │
 └──────────────────────────────────────────────────────────────┘
        │  one call per step, numbers in, numbers out
        ▼
 ┌─────────────────────────── Python ───────────────────────────┐
 │  train.py:  PPO (a standard learning method) plays the 64    │
 │             worlds, keeps what worked, saves checkpoints.    │
 │  export.py: turns the trained network into policy.json.     │
 │  eval.py:   tests Off / PD / AI on the same 1000 drops.      │
 └──────────────────────────────────────────────────────────────┘
        │
        ▼
   models/policy.json      (the trained brain; the browser game loads it in M6)
```

**Rule we never break:** all physics lives in Rust. Python only asks Rust to step the world and reads back the results. So training, testing and the game all run *the same* simulation.

## The words we use

| Word | Meaning |
|---|---|
| **Episode** | One drop: the car starts in the air, and it ends when it lands, crashes or runs out of time (8 s). |
| **Stage A** | The drop test: random height (1.5–8 m), random tilt (up to upside down) and random spin. |
| **Observation** | The 43 numbers the AI sees each step: which way is down, how fast it spins, its height, which wheels touch, what it fired last… |
| **Action** | The AI's answer each step: 10 throttles, one per thruster. |
| **Reward** | A score per step that says "good" or "bad". Landing still pays; crashing, wasting air, spinning and staying in the air cost. Simply staying alive never pays (otherwise it learns to hover). |
| **PD** | The hand-written controller from M4 (classic control maths). It is the bar the AI has to beat. |
| **Curriculum** | Start easy (small tilt, no spin), and move up a level when it succeeds more than 85% of the time. There are 5 levels. |
| **Domain randomization** | Every training drop uses a slightly different car: mass, thrust strength, wind, sensor noise, delays. So the AI can't memorise one exact car. |
| **Checkpoint** | A saved copy of the network during training. Kept in `runs/` on this Mac only (not in git). |

## How one training step works

1. Rust steps 64 worlds forward by 1/60 s, using the AI's throttles.
2. Each world reports its 43 numbers, the reward, and whether its drop ended.
3. A world whose drop ended starts a new random drop straight away.
4. Every 512 steps PPO looks at what happened and nudges the network towards the actions that scored well.
5. Every 1 million steps we save a checkpoint and test it on 200 fixed drops, keeping the best.

On this Mac that runs at about **28,000 steps per second**, so a full 50-million-step run takes about **30 minutes**.

## What happened when we trained

We did 4 runs. We watched how the car behaved, not just the score, and that caught two problems:

| Run | What we saw | What we changed |
|---|---|---|
| 001 | It landed well, but then **kept a side thruster on full** while parked. In the game that would empty the tank while you drive. | Added a cost for thrust while on the wheels. |
| 002, 003 | It found a **loophole**: it kept one wheel in the air so the "on the wheels" cost never applied. | Removed that cost. Made air 4× more expensive *everywhere* instead, so there is nothing to dodge. |
| **004** | Clean landings, no hovering, no loophole. | **This is the one we kept.** |

## The result (1000 identical test drops)

| Controller | Lands safely (Realistic) | Lands safely (Arcade) | Air per landing |
|---|---|---|---|
| Off (no thrusters) | 7% | 7% | – |
| PD (hand-written) | 58% | 61% | 36 kg |
| **AI (trained)** | **74%** | **85%** | **31 kg** |

The AI beats the hand-written controller and uses less air. We hoped for 80% / 95%, so there is still room to improve. Most of the remaining crashes are drops that start tilted and close to the ground, where nothing can react in time.

## One bug worth knowing about

The test machine (Linux) and this Mac disagreed in the very last digit of one number. The cause: some maths functions (`acos`, `sin`, `exp`…) come from the operating system and differ slightly between systems. We switched them to a library that gives the same bits everywhere, which also keeps the browser version identical.

## Where things live

| You want to… | Look at |
|---|---|
| See the numbers behind everything | `configs/*.toml` |
| Read every training run | `docs/EXPERIMENTS.md` |
| See why we decided something | `docs/DECISIONS.md` |
| Train it yourself | `docs/TRAINING.md` |
| See the trained brain | `models/policy.json` |
| See raw test results | `results/*.json` and `results/*.csv` |

## What's next (M6)

Load `policy.json` into the browser game so you can switch the controller to **AI** and watch it save the car live. We'll also add a "training viewer" that replays 8 saved checkpoints, so you can watch the cars get better over time.
