// The results screen at the finish (SPEC §11.6): time, saves, lives used and air used.
// The ghost comparison joins in M8. It only reads HUD values and writes text.

import { setText } from '../dom.ts';
import { formatTime, type HudValues } from './hud.ts';

/** The results panel and the element its lines go into. */
export interface ResultsScreen {
  panel: HTMLElement;
  lines: HTMLElement;
}

/** Shows the results of a finished run; `values` are the HUD values at the finish. */
export function showResults(screen: ResultsScreen, values: HudValues): void {
  setText(screen.lines, resultsText(values));
  screen.panel.hidden = false;
}

/** Hides the results panel. */
export function hideResults(screen: ResultsScreen): void {
  screen.panel.hidden = true;
}

/** One line each: time, saves, lives used, air used. */
export function resultsText(values: HudValues): string {
  const air = values.unlimitedAir ? '∞ (Arcade)' : `${values.airUsedKg.toFixed(1)} kg`;
  return [
    `Time ${formatTime(values.finishTimeS)}`,
    `Saves ${values.saves}`,
    `Lives used ${values.crashes}`,
    `Air used ${air}`,
  ].join('\n');
}
