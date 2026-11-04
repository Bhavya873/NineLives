// What one run did, for the results screen: saves, air used, peak g, stunts landed and
// time. It only reads the HUD values and drained events; it never touches physics. The sim's
// counts never reset (see lives.ts), so a run remembers where its counts started.
import { FLOATS_PER_EVENT } from '../sim-events.ts';
import { STUNT_NAME, StuntEvent } from '../stunt-requests.ts';
import { formatG } from './g-meter.ts';
import { formatTime, type HudValues } from './hud.ts';

/** The numbers a run collects. */
export interface RunStats {
  savesAtStart: number;
  airUsedAtStartKg: number;
  timeAtStartS: number;
  /** Stunts that landed with the full rotation, by stunt code. */
  stuntsLanded: Map<number, number>;
  /** Largest headline peak g of the run (see g-meter.ts), in g. */
  peakG: number;
  /** Largest raw contact spike of the run, in g. */
  spikeG: number;
}

/** A run starting from the sim's current counts. */
export function createRunStats(values: HudValues): RunStats {
  return {
    savesAtStart: values.saves,
    airUsedAtStartKg: values.airUsedKg,
    timeAtStartS: values.timeS,
    stuntsLanded: new Map(),
    peakG: 0,
    spikeG: 0,
  };
}

/** Starts the run over from the sim's current counts. */
export function resetRunStats(stats: RunStats, values: HudValues): void {
  stats.savesAtStart = values.saves;
  stats.airUsedAtStartKg = values.airUsedKg;
  stats.timeAtStartS = values.timeS;
  stats.stuntsLanded.clear();
  stats.peakG = 0;
  stats.spikeG = 0;
}

/** Counts each StuntLanded event in a drained batch. */
export function recordRunEvents(stats: RunStats, events: Float32Array): void {
  for (let start = 0; start < events.length; start += FLOATS_PER_EVENT) {
    if (events[start] === StuntEvent.Landed) {
      const code = events[start + 1] ?? 0;
      stats.stuntsLanded.set(code, (stats.stuntsLanded.get(code) ?? 0) + 1);
    }
  }
}

/** Keeps the run's largest headline peak and raw spike. */
export function recordRunPeaks(stats: RunStats, headlinePeakG: number, rawPeakG: number): void {
  stats.peakG = Math.max(stats.peakG, headlinePeakG);
  stats.spikeG = Math.max(stats.spikeG, rawPeakG);
}

/** Total stunts landed in the run. */
export function stuntTotal(stats: RunStats): number {
  let total = 0;
  for (const count of stats.stuntsLanded.values()) {
    total += count;
  }
  return total;
}

/** "2 Front flip, 1 Back flip", or "none". */
export function stuntBreakdown(stats: RunStats): string {
  const parts = [...stats.stuntsLanded.entries()]
    .sort((a, b) => a[0] - b[0])
    .map(([code, count]) => `${count} ${STUNT_NAME[code] ?? 'Stunt'}`);
  return parts.length > 0 ? parts.join(', ') : 'none';
}

/** The results rows, label then value, in display order. */
export function resultsRows(
  stats: RunStats,
  values: HudValues,
  bestRawPeakG: number,
): Array<[string, string]> {
  const saves = Math.max(0, values.saves - stats.savesAtStart);
  const airKg = Math.max(0, values.airUsedKg - stats.airUsedAtStartKg);
  const total = stuntTotal(stats);
  return [
    ['Saves', String(saves)],
    ['Air used', `${airKg.toFixed(1)} kg`],
    ['Peak g', `${formatG(stats.peakG)} (contact spike ${formatG(stats.spikeG)})`],
    ['Stunts landed', total === 0 ? '0' : `${total} (${stuntBreakdown(stats)})`],
    ['Time', formatTime(Math.max(0, values.timeS - stats.timeAtStartS))],
    ['Best spike ever', formatG(bestRawPeakG)],
  ];
}
