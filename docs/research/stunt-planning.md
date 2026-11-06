# Stunt planning: research and measured budgets (R1)

Research task R1 of the stunts/speed/viewer plan (G4 builds on it). This note covers four things:

- what published work says about planned flips and landing on purpose;
- the method chosen for Nine Lives;
- what the real allocator and sim can do, measured by `crates/nl-control/examples/stunt_budget.rs`;
- proposed `[stunt]` defaults. The config itself belongs to another PR.

The game's car is inspired by a 2026 roadster demo with cold-gas thrusters. Code and UI never name it.

## 1. Survey

### 1.1 Quadrotor multi-flips: adaptive flip primitives (ETH Zurich)
- **Lupashin, Schöllig, Sherback and D'Andrea, "A Simple Learning Strategy for High-Speed Quadrocopter Multi-Flips", ICRA 2010** ([PDF](https://www.dynsyslab.org/wp-content/papercite-data/pdf/lupashin-icra10.pdf)).
  - The paper splits a flip into three phases: accelerate (build up rate), drift (coast at full rate), then recover (brake and stabilise).
  - Each phase is a parameterised open-loop primitive. A first-principles model gives the first guess, and a policy-gradient step corrects the parameters between experiments.
  - The rotation in the middle runs open loop on purpose. The feedback controller takes over only for the recovery.
- **Lupashin and D'Andrea, "Adaptive Open-Loop Aerobatic Maneuvers for Quadrocopters", IFAC 2011** ([ETH Research Collection](https://www.research-collection.ethz.ch/entities/publication/eff4bc89-f799-4460-a9a5-8df4ab74851d)).
  - Journal version: "Adaptive fast open-loop maneuvers for quadrocopters", *Autonomous Robots* 33:89–102, 2012, doi:10.1007/s10514-012-9289-9 ([EPFL Infoscience](https://infoscience.epfl.ch/record/180958)).
  - A manoeuvre is a sequence of keyframes plus a parameterised input. The partial-state errors at the keyframes adapt the parameters through a first-order correction matrix taken from the model.
  - The method checks only the start and end states, never a full state trajectory.
- **What we take from it:**
  - spin-up, coast and catch as separate phases;
  - parameters come from the model (here, measured α);
  - the closed-loop controller owns the end of the manoeuvre.
- **What we do not need:** learning across trials. Our model is the sim itself, so there is no model gap to learn away.

### 1.2 Minimum-time and bang-bang attitude manoeuvres
- **Single-axis rest-to-rest turn with bounded torque.** The time-optimal control is bang-bang: full torque for half the time, then full counter-torque. For angle θ and reachable angular acceleration α, the minimum time is

  `t_min = 2·sqrt(θ/α)`, and for a full turn θ = 2π: `t_min = sqrt(8π/α)`.

  So a 2π turn fits in time t only if **α·t² ≥ 8π**. The peak rate is `ω_peak = sqrt(α·θ) = sqrt(2π·α)`.
- **Bilimoria and Wie, "Time-Optimal Three-Axis Reorientation of Rigid Spacecraft", *J. Guidance, Control, and Dynamics* 16(3), 1993.**
  - With bounded torque on each axis, the time-optimal three-axis solutions are bang-bang with 5 to 7 switches.
  - Turning about a fixed eigenaxis is not time-optimal (about 10% slower).
  - Summarised in Pager and Rao, [arXiv:2203.11394](https://arxiv.org/pdf/2203.11394).
  - For us, every stunt is a single-axis turn about a principal axis (pitch Z or roll X), so the single-axis result applies directly.
- **Rate-limited bang-coast-bang.** Spacecraft slews cap the rate so the manoeuvre stays inside rate-sensor and comfort limits: accelerate to ω_c, coast, then brake. For a full turn:

  `t(ω_c) = ω_c/α + 2π/ω_c` (spin-up plus braking at the same α, with 2π − ω_c²/α of coast).

  The air used grows with ω_c (`2·ω_c/α` seconds of firing), while time falls only slowly once ω_c > sqrt(2π·α)/2. That trade is why we coast.
- **Quadrotor follow-ups.**
  - Mueller, Hehn and D'Andrea, "A computationally efficient motion primitive for quadrocopter trajectory generation", *IEEE T-RO* 31(6), 2015. It gives closed-form primitives cheap enough to re-solve every control tick. Our per-tick replanning works the same way.
  - Mellinger, Michael and Kumar, "Trajectory generation and control for precise aggressive maneuvers with quadrotors", *IJRR* 31(5), 2012 ([PDF](https://faculty.washington.edu/minster/bio_inspired_robotics/research_articles/mellinger_michael_kumar_Trajectory_Generation_Control_aggressive_quadrotors_Jroboticsresearch2012.pdf)). It sequences controller phases (attitude, then hover) for flips and perching.

### 1.3 Rocket League bots: aerial recovery
Rocket League bots face our exact problem: a car in ballistic flight with bounded angular acceleration must land on its wheels.
- **VirxERLU `recovery` routine** ([util/routines.py](https://github.com/VirxEC/VirxERLU/blob/master/util/routines.py), [util/utils.py `find_landing_plane`](https://github.com/VirxEC/VirxERLU/blob/master/util/utils.py)).
  - It solves the ballistic arc against every arena surface: a linear time for walls, and the quadratic `z(t) = z0 + vz·t − g·t²/2` for floor and ceiling.
  - The surface hit first is the landing plane. A PD attitude law then points the car's up axis along that plane's normal and its nose along the velocity.
  - Re-run every tick, it is a time-to-impact forecast with replanning.
- **RLUtilities `Reorient` / `AerialTurn` (Sam Mish)** ([inc/mechanics/reorient.h](https://github.com/samuelpmish/RLUtilities/blob/master/inc/mechanics/reorient.h), [ExampleBots 2_Aerial_Recovery](https://github.com/samuelpmish/ExampleBots/tree/master/2_Aerial_Recovery)).
  - It models the car's per-axis angular acceleration and damping, simulates the turn ahead over a `horizon_time`, and finishes within the tolerances `eps_phi` and `eps_omega`.
  - The example bot throws the car into random spins and scores the recovery. That is our refusal and abort test, in miniature.
- **What we take from it:**
  - Forecast touchdown from the ballistic arc and the actual geometry (here, a ray/shape cast on the course).
  - Aim the car's up axis at the landing normal, not at world up.
  - Re-plan every tick.
  - Measure success by tolerances on angle and rate at touchdown.

### 1.4 Time to impact and landing-surface estimation
- **Lee, D. N. (1976), "A theory of visual control of braking based on information about time-to-collision", *Perception* 5(4):437–459.** It introduced τ = distance / closing speed as the quantity animals use to time braking and landing.
- **Robotic uses:**
  - tau-based landing controllers for quadrotors (Alkowatly, Becerra and Holderbaum, *JGCD* 2015);
  - flip-to-perch triggers based on time to contact ([arXiv:2412.19765](https://arxiv.org/pdf/2412.19765), which defines it as distance over the velocity normal to the surface).
- **Our case is easier.** We know the exact state and the geometry, so the forecast is a ballistic integration (with drag, through the sim) against the course colliders. τ is useful only as a cheap first check: `τ ≈ h / v_down` before the full forecast.
- **Uncertainty.** We bound it by sampling the forecast at ±2% speed and ±1° pitch (plan G4), as a stand-in for unmodelled drag, thrust side-forces and valve lag.

### 1.5 Thruster-assisted car flight and reorientation
- **Public demo.** No peer-reviewed study of thruster-stabilised car flight or flips turned up. Coverage of the 2026 roadster demo describes 10 cold-gas nozzles, a tank of about 260 kg of air, and closed-loop attitude control in the air. The control algorithms are not disclosed ([example coverage](https://interestingengineering.com/videos/teslas-roadster-hangs-upside-down-and-flies-heres-how-it-actually-works)).
- **Simulation only.** A BeamNG.drive mod ([Rocket Pack: Kickflipping Ardente](https://beamng.com/resources/rocket-pack-kickflipping-ardente-with-auto-land.39818/)) does the same in simulation, with an "auto-land".
- **Closest experimental research: inertial tails rather than thrusters.**
  - Libby et al., "Tail-assisted pitch control in lizards, robots and dinosaurs", *Nature* 481, 2012.
  - A 3-DoF inertial tail that lands a falling quadruped ([arXiv:2209.15337](https://arxiv.org/pdf/2209.15337)).
  - Their lesson matches ours: reorienting in the air is limited by the angular acceleration available and by the time to impact, and the landing attitude, not the turn, decides survival.
- **Cold-gas thrusters for attitude control** are a mature spacecraft technology. Our air model (`ṁ = F / v_e`, v_e = 600 m/s) is the standard one.

## 2. Chosen method (feeds plan G4)

### 2.1 Phases and nozzle ownership
`Idle → Requested → Authorized → SpinUp → Coast → Brake → Catch → Settled | Aborted`

| Phase | Owner | What it does |
|---|---|---|
| SpinUp | StuntDirector | Asks the allocator for full torque `τ = sign·I_axis·α_big` about the stunt axis (body Z for flips, body X for barrels) until `ω_axis ≥ ω_coast`. |
| Coast | StuntDirector | Asks for nothing (the valves close with their lag). Integrates `θ += ω_axis·dt` at 120 Hz. |
| Brake | StuntDirector | Applies full counter-torque once the remaining angle `2π − θ` reaches the switching curve, down to `ω_handover`. |
| Catch | PD | Plain PD, unchanged and bit-identical. It finishes the last few degrees and lines up the landing. |

- **Exactly one owner each control tick.** The executor asserts it.
- **The Hybrid gate is held to PD** from Authorized until 1 s after Settled, whatever `enter_tilt_deg` says.
- **Change to plan §1.5:** a **Brake** phase is added, owned by the StuntDirector. Section 3 shows why: handing PD the full coast rate costs up to twice the air and, above about 5 rad/s in pitch, an extra second of catch time. The plan's air budget already listed "braking" as its own item.

### 2.2 Formulas
- **α:** the measured reachable angular acceleration (Section 3) per mode and axis, times the tank gate.
- **Spin-up:** `t_up = ω_c/α`, `θ_up = ω_c²/(2α)`.
- **Brake (switching curve):** start braking when `2π − θ ≤ (ω² − ω_h²)/(2α) + ω·τ_valve`, with τ_valve = 0.03 s.
  - Replanned per tick as the feedback law `ω_wanted(θ) = sqrt(ω_h² + 2α·(2π − θ))`: brake whenever ω exceeds it.
  - This fixes the fixed-lead error measured at high rates (Section 3.3).
- **Coast:** `t_coast = (2π − θ_up − θ_brake)/ω_c`. It needs `θ_up + θ_brake ≤ 2π`, i.e. `ω_c ≤ sqrt(2π·α)` roughly.
- **Feasibility in time:**

  `t_avail − Δt_uncertainty ≥ t_up + t_coast + t_brake + t_catch + t_settle_margin`.

  The bang-bang lower bound `α·t² ≥ 8π` is a quick reject before any forecast.
- **Coast-rate choice:** the smallest ω_c from the config list with `t(ω_c) ≤ t_avail − Δt − t_catch − margin`. A slower coast uses less air, because air ∝ firing time = `2·ω_c/α`.
- **Air:** `air_needed = margin × (ṁ_axis·(t_up + t_brake) + air_catch) + landing_reserve`. Here ṁ_axis is measured (40–107 kg/s, Section 3.1). The landing reserve is the largest PD landing air measured on the playground. Arcade has unlimited air, so only the time checks apply there.
- **Completion:** `θ ≥ 2π − 0.15` from the integrated body rate, never "upright at the end".
- **Abort:** if a replanned forecast says `t_avail` has dropped below `t_brake(ω) + t_catch + margin`, brake at once and hand to PD (recovery first). This emits StuntAborted.

### 2.3 Authorization (conservative)
All of these must hold at request time and again every tick until Brake:
1. **Landing surface:** the forecast normal is within 15° of up, and the usable length (flat enough, no obstacle) is at least 2 car lengths (8.4 m).
2. **Time:** the time inequality above holds, using the ±2% speed / ±1° pitch sample spread as Δt.
3. **Air:** the air inequality above holds (Realistic).
4. **Start envelope:**
   - the car is airborne (past AirborneStart: the interlock and the wheels are no longer involved);
   - tilt ≤ 25°;
   - spin ≤ 1.5 rad/s;
   - tank gate = 1;
   - mode authority known.

## 3. Measured (2026-10-27, `cargo run --release -p nl-control --example stunt_budget`)

**Method:**
- Everything goes through the real actuator-aware allocator (`Allocator::solve_actuator_aware`) and the real `World`. Nothing is re-implemented.
- **α:** the slope of ω between 0.15 and 0.45 s of saturated torque, measured 300 m up.
- **Flips:** the car is thrown up so that the stop point comes at the top of the arc. That keeps PD's fall braking out of the catch figure, because that air belongs to the landing.
- **"Landable":** PD's own quiet limits (tilt < 15°, spin < 1.5 rad/s).
- **"Settled":** tilt < 5° and spin < 0.5 rad/s.

**Raw data:** `results/stunt_budget.json`.

### 3.1 Reachable angular acceleration

| Mode | Axis | α rad/s² | Air while firing kg/s | Bang-bang 2π, s |
|---|---|---|---|---|
| Realistic | pitch (front/back flip) | 13.2 | 40.0 | 1.38 |
| Realistic | roll (barrel L/R) | 28.7 | 53.3 | 0.94 |
| Arcade | pitch | 26.5 | 80.0 | 0.97 |
| Arcade | roll | 57.4 | 106.6 | 0.66 |

- Front and back, and left and right, are symmetric to 0.1 rad/s².
- PD's allocation weights (force 1, torque 3) reach the same α as torque-only weights (force 0). The nozzles form pure couples, so asking for zero net force costs no authority.
- Inertia is [850, 3250, 2900] kg·m² (roll X, yaw Y, pitch Z) and the mass is 1800 kg, as assumed.

### 3.2 Full turn: time and air until landable, Brake → PD (recommended)

| Mode | Axis | ω_c 3 | 4 | 5 | 6 | 8 |
|---|---|---|---|---|---|---|
| Realistic | flip | 2.12 s / 15.6 kg | 1.73 / 21.3 | **1.55 / 27.6** | 1.43 / 33.3 | 1.33 / 45.3 |
| Realistic | barrel | 1.78 / 10.6 | 1.50 / 14.1 | **1.28 / 18.1** | 1.82* / 26.6 | 1.73* / 34.0 |
| Arcade | flip (back) | 1.82 / 16.6 | 1.50 / 23.8 | **1.33 / 29.2** | 1.18 / 35.2 | 1.70* / 52.5 |
| Arcade | barrel | 1.43 / 12.6 | 1.23 / 16.0 | 1.08 / 19.5 | **0.98 / 23.0** | 0.85 / 30.0 |

\* The brake handed over with tilt still above 15°, and PD needed another 0.6–0.7 s. This is the fixed-lead error that per-tick replanning (§2.2) removes. Arcade front flip at 6 rad/s: 1.83 s* (back flip 1.18 s).

- **Realistic, 10 rad/s:** cannot be reached and stopped within one pitch turn.
- **Settling** to < 5° and < 0.5 rad/s takes a further 1.0–1.3 s after hand-over in every case. So `t_catch` must use "landable", not "settled".

### 3.3 PD-only catch (plan §1.5 as written), for comparison

| Mode | Axis | ω_c 4 | 5 | 6 | 8 |
|---|---|---|---|---|---|
| Realistic | flip | 1.75 s / 21.3 kg | 1.55 / 27.2 | 2.48 / 41.6 | 2.37 / 63.8 |
| Realistic | barrel | 2.40 / 25.5 | 2.20 / 34.3 | 2.17 / 38.4 | 2.17 / 47.0 |
| Arcade | flip | 2.42 / 28.7 | 2.22 / 33.9 | 2.18 / 41.0 | 2.23 / 54.1 |
| Arcade | barrel | 2.35 / 34.6 | 2.25 / 40.8 | 2.17 / 45.2 | 2.22 / 54.6 |

- PD-only matches Brake → PD only for Realistic flips at ≤ 5 rad/s.
- Everywhere else it is 0.5–1.1 s slower and uses up to twice the air. PD's tuned roll and tumble gains damp slowly, and its 25 rad/s² comfort cap is not what the allocator can deliver in pitch (13.2), so its stopping estimate is off.

### 3.4 Playground ramp airtime (Realistic, cruise-controlled approach, 25 m run-up)

The "nozzles off" airtime is ballistic. With PD flying, the airtime grows by 0.15–0.5 s because PD brakes the fall. Landing air is the air PD uses from take-off to 0.5 s after touchdown. The PD crash column notes the speeds where PD crashed (HardImpact).

| Ramp (piece) | 15 m/s | 20 m/s | 25 m/s | 30 m/s | PD landing air kg (15→30) | PD crash |
|---|---|---|---|---|---|---|
| 12 m 18° (#1, #30) | 1.13 / 0.82 s | 1.43 | 1.76 | 2.08 | 12–37 | #1 at 25, 30; #30 at 30 |
| 10 m 15° (#3, #28: car rows) | 0.64 | 0.98 | 1.46 | 1.75 | 6–28 | #3 at 30 |
| 10 m 15° (#20, #26, #41, #43) | 0.73–0.91 | 1.18 | 1.46 | 1.75 | 7–29 | none |
| 15 m 22° (#9, #22) | 1.40 | 1.78 | 2.15 | 2.56 | 26–53 | #9 at 30; #22 at 20 |
| 15 m 28° (#24) | 1.72 | 2.18 | 2.65 | 3.05 | 35–57 | at 30 |
| 10 m 18° (#42, #44, diagonal) | 1.08 | 1.41 | 1.73 | 2.02 | 17–36 | none |

**Which stunts fit** (ballistic airtime ≥ manoeuvre time + 0.3 s margin, Realistic):
- **Flip at 5 rad/s (1.85 s):**
  - #9 and #22 at ≥ 25 m/s;
  - #24 at ≥ 20 m/s;
  - #1 and #30 only at 30 m/s, where PD itself crashes on #1.
- **Barrel at 5 rad/s (1.58 s):**
  - #9 and #22 at ≥ 20 m/s;
  - #24 at ≥ 15 m/s;
  - #1, #30, #42 and #44 at ≥ 25 m/s.
- **Small 15° ramps:** nothing fits below 30 m/s.

**Thruster interlock:** it held for 0.00 s after AirborneStart on every ramp run. Once the car is airborne, the interlock never blocks a stunt. Before take-off it blocks everything, so a stunt can only start after AirborneStart, as the start envelope says.

### 3.5 Surprises
1. **The allocator delivers the expected α.** Asking for zero net force does not reduce it.
2. **PD-only catch is the weak link.** It is the reason for the director-owned Brake phase (§2.1).
3. **A fixed brake lead stops short at high rates.** This happens at ≥ 6 rad/s (Realistic barrel, Arcade) and costs +0.6 s. A per-tick switching-curve brake is required, not optional.
4. **The plan's "Realistic flip ≈ 1.4 s, 55 kg" is the bang-bang figure.** A 5 rad/s coast takes 1.55 s for half the air (28 kg).
5. **PD crashes on landing on several ramps at 25–30 m/s, and on #22 at 20 m/s.** These are landing-zone or geometry crashes (HardImpact), not attitude failures. They belong to G2's survival contract, but the stunt planner must also refuse wherever the forecast landing is not "usable" (§2.3.1).

## 4. Proposed `[stunt]` defaults (for the G0/G4 config PR; not committed to `configs/` here)

```toml
[stunt]
# Coast rate per stunt and mode, rad/s. stunt_budget Brake→PD rows: cheapest rate whose
# time to landable is within ~0.2 s of the fastest measured (docs/research/stunt-planning.md §3.2).
flip_coast_rate_realistic_rad_s = 5.0     # 1.55 s, 28 kg
barrel_coast_rate_realistic_rad_s = 5.0   # 1.28 s, 18 kg
flip_coast_rate_arcade_rad_s = 5.0        # 1.33 s
barrel_coast_rate_arcade_rad_s = 6.0      # 0.98 s
# Reachable angular acceleration through the allocator at full tank, rad/s² (§3.1); the
# planner multiplies by the mode's thrust scale (Arcade = 2x) and the tank gate.
pitch_alpha_rad_s2 = 13.2
roll_alpha_rad_s2 = 28.7
# Air while firing full stunt torque, kg/s at thrust scale 1 (§3.1).
pitch_air_kg_s = 40.0
roll_air_kg_s = 53.3
# The Brake phase hands over to PD at this rate (= pd.quiet_max_spin_rad_s).
handover_rate_rad_s = 1.5
# Catch budget after hand-over, s (worst measured hand-over-to-landable, rounded up).
catch_time_s = 0.8
catch_air_kg = 11.0
# "Complete" needs at least 2π − this, rad.
completion_slack_rad = 0.15
settle_margin_s = 0.3
# Authorization (plan G4).
max_landing_normal_tilt_deg = 15.0
min_usable_landing_m = 8.4                # two car lengths
forecast_speed_spread = 0.02
forecast_pitch_spread_deg = 1.0
air_margin = 1.25
landing_air_kg_per_m_s = 6.0              # replaced the flat 60 kg reserve (2026-11-05)
start_max_tilt_deg = 25.0
start_max_spin_rad_s = 1.5
# Player request handling.
request_expiry_s = 3.0
hybrid_hold_after_settled_s = 1.0
```

The worst Realistic flip budget is `1.25 × (27.6 + 11) + 60 ≈ 108 kg`. That is about 40% of the 260 kg tank, so a flip needs a tank at least 42% full.

## Sources
- Lupashin, Schöllig, Sherback, D'Andrea, ICRA 2010: https://www.dynsyslab.org/wp-content/papercite-data/pdf/lupashin-icra10.pdf
- Lupashin and D'Andrea, IFAC 2011: https://www.research-collection.ethz.ch/entities/publication/eff4bc89-f799-4460-a9a5-8df4ab74851d; *Autonomous Robots* 2012: https://infoscience.epfl.ch/record/180958
- Bilimoria and Wie, JGCD 1993, summarised in Pager and Rao: https://arxiv.org/pdf/2203.11394
- Mellinger, Michael and Kumar, IJRR 2012: https://faculty.washington.edu/minster/bio_inspired_robotics/research_articles/mellinger_michael_kumar_Trajectory_Generation_Control_aggressive_quadrotors_Jroboticsresearch2012.pdf
- Mueller, Hehn and D'Andrea, IEEE T-RO 2015, doi:10.1109/TRO.2015.2479878
- VirxERLU recovery: https://github.com/VirxEC/VirxERLU
- RLUtilities Reorient and ExampleBots: https://github.com/samuelpmish/RLUtilities, https://github.com/samuelpmish/ExampleBots
- Lee 1976, *Perception* 5(4):437–459; time-to-contact perching: https://arxiv.org/pdf/2412.19765
- Inertial-tail landing: https://arxiv.org/pdf/2209.15337; Libby et al., *Nature* 481, 2012
- Thruster-car demo coverage: https://interestingengineering.com/videos/teslas-roadster-hangs-upside-down-and-flies-heres-how-it-actually-works; BeamNG mod: https://beamng.com/resources/rocket-pack-kickflipping-ardente-with-auto-land.39818/
