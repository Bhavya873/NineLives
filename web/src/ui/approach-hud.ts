// The approach HUD: an amber speed readout above 200 km/h, a "too slow" warning when
// the sim raises TooSlowFallback (event 16), and an "AI outside envelope" badge when
// HudField PolicyOutOfEnvelope is 1. Before a piece, HudField 26 (ApproachHint) holds the
// entry speed the piece ahead needs while the car is too slow for it, and the HUD names
// it; after the entry, TooSlowFallback keeps the warning up for a few seconds. Pure logic.
import { FLOATS_PER_EVENT } from '../wasm-bridge/sim-events.ts';

/** Event kinds this HUD reads (docs/contracts/events-hud.md). */
const APPROACH_EVENT = { Scrape: 15, TooSlowFallback: 16 } as const;

/** Amber above this speed: 200 km/h. */
export const FAST_SPEED_M_S = 200 / 3.6;
/** How long the too-slow warning stays. */
const TOO_SLOW_SHOW_S = 3;

/** True when the speed readout should be amber. */
export function isFast(speedMS: number): boolean {
  return Math.abs(speedMS) > FAST_SPEED_M_S;
}

/** True when the AI badge should show (HudField PolicyOutOfEnvelope === 1). */
export function outsideEnvelope(flag: number): boolean {
  return flag === 1;
}

/** The pre-entry hint for HudField ApproachHint (needed speed in m/s, 0 = none), or ''. */
export function approachHintText(neededSpeedMS: number): string {
  if (!(neededSpeedMS > 0)) {
    return '';
  }
  return `Too slow to clear this piece: need ${Math.ceil(neededSpeedMS * 3.6)} km/h`;
}

/** The warning text for TooSlowFallback events in a batch (the last one), or null. */
export function tooSlowWarning(events: Float32Array): string | null {
  let text: string | null = null;
  for (let start = 0; start < events.length; start += FLOATS_PER_EVENT) {
    if (events[start] === APPROACH_EVENT.TooSlowFallback) {
      const kmH = Math.abs(events[start + 2] ?? 0) * 3.6;
      text = `Too slow to clear this piece (${kmH.toFixed(0)} km/h)`;
    }
  }
  return text;
}

/** Normal speeds (m/s) of the Scrape events in a batch, in order. */
export function scrapeSpeeds(events: Float32Array): number[] {
  const speeds: number[] = [];
  for (let start = 0; start < events.length; start += FLOATS_PER_EVENT) {
    if (events[start] === APPROACH_EVENT.Scrape) {
      speeds.push(Math.abs(events[start + 1] ?? 0));
    }
  }
  return speeds;
}

/** The warning's state. */
export interface TooSlowState {
  text: string;
  hideAtS: number;
}

export function newTooSlowState(): TooSlowState {
  return { text: '', hideAtS: 0 };
}

/** Starts the warning from a batch; returns the text to show at `nowS` ('' = hidden). */
export function updateTooSlow(state: TooSlowState, events: Float32Array, nowS: number): string {
  const warning = tooSlowWarning(events);
  if (warning !== null) {
    state.text = warning;
    state.hideAtS = nowS + TOO_SLOW_SHOW_S;
  }
  return nowS < state.hideAtS ? state.text : '';
}
