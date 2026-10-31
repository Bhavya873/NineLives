// The session's g records (docs/contracts/g-meter.md): the best raw peak g, kept in
// localStorage under `ninelives.g.v1`, and the best g per stunt for this session.
// Shown for a few seconds after a landing. Every storage access is in try/catch, and an
// unknown or older stored shape is dropped.
import { FLOATS_PER_EVENT } from '../sim-events.ts';
import { STUNT_NAME, StuntEvent } from '../stunt-requests.ts';

export const G_RECORD_KEY = 'ninelives.g.v1';
export const G_RECORD_VERSION = 1;
/** How long the record line stays after a landing. */
export const G_RECORD_SHOW_S = 4;

/** The slice of `Storage` the record needs. */
export interface GStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export interface GRecord {
  /** Best raw peak g ever (persisted). */
  bestPeakG: number;
  /** Best peak g per stunt code, this session only. */
  perStunt: Map<number, number>;
  /** Time the record line hides at, in seconds; 0 when hidden. */
  showUntilS: number;
  /** Text of the line to show. */
  text: string;
  storage: GStorage | null;
}

/** Reads the stored best; anything but the current shape gives 0. */
export function loadBestPeakG(storage: GStorage | null): number {
  try {
    const raw = storage?.getItem(G_RECORD_KEY);
    if (!raw) {
      return 0;
    }
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== 'object' || parsed === null) {
      return 0;
    }
    const { bestPeakG, version } = parsed as { bestPeakG?: unknown; version?: unknown };
    if (version !== G_RECORD_VERSION || typeof bestPeakG !== 'number') {
      return 0;
    }
    return Number.isFinite(bestPeakG) && bestPeakG > 0 ? bestPeakG : 0;
  } catch {
    return 0;
  }
}

function saveBestPeakG(storage: GStorage | null, bestPeakG: number): void {
  try {
    storage?.setItem(G_RECORD_KEY, JSON.stringify({ bestPeakG, version: G_RECORD_VERSION }));
  } catch {
    // Storage may be blocked; the record then lasts for the session only.
  }
}

/** A record starting from what `storage` holds (may be null). */
export function createGRecord(storage: GStorage | null): GRecord {
  return {
    bestPeakG: loadBestPeakG(storage),
    perStunt: new Map(),
    showUntilS: 0,
    text: '',
    storage,
  };
}

/** Feeds the live raw peak (HudField PeakGLoad); a new best is stored at once. */
export function recordPeak(record: GRecord, peakG: number): void {
  if (Number.isFinite(peakG) && peakG > record.bestPeakG) {
    record.bestPeakG = peakG;
    saveBestPeakG(record.storage, peakG);
  }
}

/**
 * Reads a batch of events: each StuntLanded updates the stunt's best and starts the
 * record line. Returns true if a landing was seen.
 */
export function recordLandings(record: GRecord, events: Float32Array, nowS: number): boolean {
  let landed = false;
  for (let start = 0; start < events.length; start += FLOATS_PER_EVENT) {
    if (events[start] !== StuntEvent.Landed) {
      continue;
    }
    const code = events[start + 1] ?? 0;
    const peakG = events[start + 2] ?? 0;
    recordPeak(record, peakG);
    const stuntBest = Math.max(record.perStunt.get(code) ?? 0, peakG);
    const isNewStuntBest = peakG >= (record.perStunt.get(code) ?? 0);
    record.perStunt.set(code, stuntBest);
    const name = STUNT_NAME[code] ?? 'Stunt';
    record.text =
      `${name} best ${stuntBest.toFixed(1)} g${isNewStuntBest ? ' (new)' : ''}` +
      ` · session best ${record.bestPeakG.toFixed(1)} g`;
    record.showUntilS = nowS + G_RECORD_SHOW_S;
    landed = true;
  }
  return landed;
}

/** The line to show at `nowS`, or '' when hidden. */
export function recordLine(record: GRecord, nowS: number): string {
  return nowS < record.showUntilS ? record.text : '';
}

/** The browser's localStorage, or null when blocked. */
export function browserStorage(): GStorage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}
