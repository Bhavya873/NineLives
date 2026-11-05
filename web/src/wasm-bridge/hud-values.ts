// Reads the HUD numbers out of the flat array that `Game.hud()` returns.
// One reader for the whole page, so the field order lives in one place (the wasm `HudField`).
import { type HudValues } from '../ui/hud.ts';
import { ControllerCode, HudField, ModeCode } from '../wasm/nl_wasm.js';

/** A HudValues at the start of a run, before the first read from the game. */
export function newHudValues(): HudValues {
  return {
    speedMS: 0,
    mode: ModeCode.Realistic,
    controller: ControllerCode.Off,
    airFraction: 1,
    unlimitedAir: false,
    hopBlocked: 0,
    crashed: false,
    saves: 0,
    crashes: 0,
    timeS: 0,
    hopCooldownS: 0,
    wheelsTouching: 4,
    outOfAir: false,
    airUsedKg: 0,
    checkpointsReached: 0,
    checkpointCount: 0,
    finished: false,
    finishTimeS: 0,
    gLoad: 1,
    peakG: 0,
    policyOutOfEnvelope: 0,
    approachHintMS: 0,
    windWorldM_S: [0, 0, 0],
  };
}

/** Copies the values the HUD shows out of `Game.hud()`'s array into `out`. */
export function fillHudValues(values: Float32Array, out: HudValues): void {
  out.speedMS = values[HudField.SpeedMS] ?? 0;
  out.mode = values[HudField.Mode] ?? ModeCode.Custom;
  out.controller = values[HudField.Controller] ?? ControllerCode.Off;
  out.airFraction = values[HudField.AirFraction] ?? 1;
  out.unlimitedAir = values[HudField.UnlimitedAir] === 1;
  out.hopBlocked = values[HudField.HopBlocked] ?? 0;
  out.crashed = values[HudField.Crashed] === 1;
  out.saves = values[HudField.Saves] ?? 0;
  out.crashes = values[HudField.Crashes] ?? 0;
  out.timeS = values[HudField.TimeS] ?? 0;
  out.hopCooldownS = values[HudField.HopCooldownS] ?? 0;
  out.wheelsTouching = values[HudField.WheelsTouching] ?? 0;
  out.outOfAir = values[HudField.OutOfAir] === 1;
  out.airUsedKg = values[HudField.AirUsedKg] ?? 0;
  out.checkpointsReached = values[HudField.CheckpointsReached] ?? 0;
  out.checkpointCount = values[HudField.CheckpointCount] ?? 0;
  out.finished = values[HudField.Finished] === 1;
  out.finishTimeS = values[HudField.FinishTimeS] ?? 0;
  out.gLoad = values[HudField.GLoad] ?? 1;
  out.peakG = values[HudField.PeakGLoad] ?? 0;
  out.policyOutOfEnvelope = values[HudField.PolicyOutOfEnvelope] ?? 0;
  out.approachHintMS = values[HudField.ApproachHint] ?? 0;
  out.windWorldM_S[0] = values[HudField.WindXMS] ?? 0;
  out.windWorldM_S[2] = values[HudField.WindZMS] ?? 0;
}
