# Contract: stunt state machine and nozzle ownership

Frozen at Gate 0 (plan `docs/plans/2026-10-27-stunts-speed-viewer.md` §1.5). Built in G4 (`claude/stunt-forecast`, `claude/stunt-keys`, `claude/stunt-integrate`). The phase list below is as built: R1's braking budget added a `Brake` phase.

## Phases
```text
Idle → Requested → Authorized → SpinUp → Coast → Brake → Catch → Settled
                                    \________\________\→ Aborted
```
- **Idle:** no stunt.
- **Requested:** the player pressed a stunt key (Q/E barrel roll left/right, Z front flip, B back flip). A request not authorized within 3 s expires. A request made while the car is not yet airborne, or is still tilted past the start limit (a steep ramp launches it nose up while PD levels it), waits in Requested instead of being refused; any other failed rule refuses it at once.
- **Authorized:** the planner checked the landing forecast, time and air, and said yes. Otherwise it emits `StuntRefused` and returns to Idle.
- **SpinUp:** nozzles push to build the spin.
- **Coast:** the car rotates on its own momentum.
- **Brake:** the director fires the counter-torque so the spin ends before touchdown.
- **Catch:** PD stops any remaining spin and lands the car upright.
- **Settled:** on the wheels, stable. Emits `StuntLanded`.
- **Aborted:** the planner, replanning every control tick, saw the landing window shrink below the catch time. It hands over to Catch (recovery first) and emits `StuntAborted`.

A stunt is **complete** only if the integrated rotation about the stunt axis is ≥ 2π − 0.15 rad. Being upright at the end is not enough. Completion is **latched**: once the integrated rotation first reaches 2π − 0.15 rad it stays complete for the rest of the stunt, so PD's catch turning a ramp-launched car back to level (which lowers the counted rotation again) cannot cancel a turn already made. The latch clears when the stunt ends (Idle, `reset`) or a new one is authorized. A stunt aborted before the turn completed never reports `StuntLanded`.

## Who owns the nozzles
Exactly **one** owner each tick:

| Phase | Owner |
|---|---|
| SpinUp, Coast, Brake | StuntDirector |
| Catch, Settled, Aborted (recovery) | PD |

The Hybrid gate is held to PD for the whole stunt and for **1 s** after `Settled`, so the AI never takes over mid-stunt. Ownership is written to the HUD (`StuntPhase`) and to the event stream (see `events-hud.md`).

## Numbers (as built)
- **`StuntPhase`** (HUD 24): 0 Idle, 1 Requested, 2 Authorized, 3 SpinUp, 4 Coast, 5 Brake, 6 Catch, 7 Settled, 8 Aborted.
- **`StuntCode`** (HUD 25, and float 1 of every stunt event): 1 barrel roll left, 2 barrel roll right, 3 front flip, 4 back flip; 0 means none.
- **Events** (5 floats): `StuntStart` `[11, code, 0, 0, 0]`; `StuntLanded` `[12, code, peak_g, rotation_rad, 0]`; `StuntRefused` `[13, code, reason, 0, 0]`; `StuntAborted` `[14, code, reason, rotation_rad, 0]`.
- **Reason:** 0 not enough air time (time, air, start state), 1 landing blocked (no landing, tilted, too short), 2 request expired (no take-off within the timeout), 3 control lost (aborted in the air).
- **Request:** the wasm `Game.request_stunt(code)` queues one request and returns false for an unknown code, a crashed car or a stunt already queued or flying; the planner decides at the next control step in the air.
- **Ready mask (read-only):** the wasm `Game.stunt_ready_mask()` runs the planner's four rules (`authorize`) for each stunt on a fresh landing forecast and returns a bit mask, bit `code - 1` set when that stunt would be authorized now. It returns 0 on the ground, while crashed or while a stunt is queued or flying, starts nothing and changes no sim state. It costs one forecast, so the web asks only while airborne, to show "Stunt ready: Q E Z B".
- The executor is `nl_control::stunt::StuntDirector`: it runs the planner each control tick, turns the spin command into throttles through PD's allocation, and PD flies the catch. With no stunt requested it does nothing, so stunt-free behaviour (and the golden trace) is unchanged.

## `[stunt]` settings
The `[stunt]` table in `configs/controllers.toml` (strict parser in `crates/nl-control/src/config.rs`) holds the values from R1 (`docs/research/stunt-planning.md`). The entries below are the ones the contract names, all *(start)*:
- `request_timeout_s = 3.0`: how long a queued request waits.
- `hybrid_hold_after_settled_s = 1.0`: how long the Hybrid gate stays on PD after Settled.
- `complete_rotation_slack_rad = 0.15`: rotation short of 2π still counted complete.
- `landing_max_normal_tilt_deg = 15.0`, `landing_min_length_car_lengths = 2.0`: landing surface checks.
- spin-up, braking, catch and landing-reserve air budgets per axis and mode (from R1).

## Tests that pin this (plan §3)
- **Ownership:** exactly one nozzle owner each tick (asserted in the executor). Hybrid gate held throughout. PD output bit-identical with stunts off.
- **Stunt success:** fixed cases (e.g. playground ramp at x = 310, 22 m/s, full tank, Realistic, PD): rotation ≥ 2π − 0.15, stable touchdown, no crash in a 1.5 s settling window, heading error < 30°.
- **Sign tests:** front flip nose down, back flip nose up, barrel right rolls right side down.
- **Stunt refusal:** refused for low air, existing spin above the start envelope, a blocked landing, too little braking time, or a short airtime. An abort mid-stunt when the landing shrinks ends in a recovered landing.
