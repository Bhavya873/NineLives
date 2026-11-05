// The g-meter (docs/contracts/g-meter.md): a small gauge showing the live passenger g
// (HudField GLoad, already smoothed over 0.1 s by the sim) and the raw peak g of the
// current stunt window (HudField PeakGLoad). It replaces the old screen blackout and
// never darkens the view. The maths is pure so tests run without a browser.

/** The scale's top, in g; the needle pins here. */
const G_METER_MAX_G = 8;
/** From this g the readout is amber; it matches the reward's soft limit (3 g). */
const G_METER_WARN_G = 3;
/** From this g the readout is red. */
const G_METER_HIGH_G = 6;

type GLevel = 'calm' | 'warn' | 'high';

/** Needle position, 0 (0 g) to 1 (full scale). Bad numbers read as 0. */
export function gNeedleFraction(g: number): number {
  if (!Number.isFinite(g) || g <= 0) {
    return 0;
  }
  return Math.min(1, g / G_METER_MAX_G);
}

/** How alarming `g` is. */
export function gLevel(g: number): GLevel {
  if (g >= G_METER_HIGH_G) {
    return 'high';
  }
  return g >= G_METER_WARN_G ? 'warn' : 'calm';
}

/** One decimal, e.g. "3.4 g"; bad numbers read as "0.0 g". */
export function formatG(g: number): string {
  return `${(Number.isFinite(g) ? Math.max(0, g) : 0).toFixed(1)} g`;
}

/**
 * The headline peak g (docs/contracts/g-meter.md, UI note 2026-11-03). The sim's raw peak
 * can be one 120 Hz contact tick (a drop test reads 39 g), which says more about the
 * solver than the car. The headline is the largest displayed (0.1 s smoothed) g since the
 * raw peak last reset, capped by the raw peak itself. The raw peak stays in HudField 23
 * and in the Save event untouched.
 */
export interface GPeakTracker {
  /** Largest displayed g since the current stunt window opened, in g. */
  windowMaxG: number;
  /** The raw peak seen last frame, to notice a reset. */
  lastRawPeakG: number;
  /** What the meter shows as the peak, in g. */
  headlineG: number;
}

export function createGPeakTracker(): GPeakTracker {
  return { windowMaxG: 0, lastRawPeakG: 0, headlineG: 0 };
}

/** Feeds one frame's displayed g and the raw peak; returns the headline peak. */
export function updateGPeak(tracker: GPeakTracker, liveG: number, rawPeakG: number): number {
  const raw = Number.isFinite(rawPeakG) ? Math.max(0, rawPeakG) : 0;
  if (raw < tracker.lastRawPeakG - 1e-3) {
    // The raw peak dropped: a new stunt window opened (take-off, respawn or placement).
    tracker.windowMaxG = 0;
  }
  tracker.lastRawPeakG = raw;
  if (Number.isFinite(liveG)) {
    tracker.windowMaxG = Math.max(tracker.windowMaxG, liveG);
  }
  tracker.headlineG = Math.min(raw, tracker.windowMaxG);
  return tracker.headlineG;
}

/** The meter's elements. */
export interface GMeter {
  root: HTMLElement;
  fill: HTMLElement;
  live: HTMLElement;
  peak: HTMLElement;
  lastKey: string;
}

/** Wraps the page's meter elements. */
export function createGMeter(
  root: HTMLElement,
  fill: HTMLElement,
  live: HTMLElement,
  peak: HTMLElement,
): GMeter {
  return { root, fill, live, peak, lastKey: '' };
}

/**
 * Draws live g and the peak; skips the DOM when nothing changed. `peakG` is the headline
 * peak (see `updateGPeak`); `rawPeakG`, if different, goes in the tooltip.
 */
export function updateGMeter(
  meter: GMeter,
  liveG: number,
  peakG: number,
  rawPeakG: number = peakG,
): void {
  const liveText = formatG(liveG);
  const peakText = formatG(peakG);
  const rawText = formatG(rawPeakG);
  const level = gLevel(liveG);
  const width = `${(gNeedleFraction(liveG) * 100).toFixed(0)}%`;
  const key = `${liveText}|${peakText}|${rawText}|${level}|${width}`;
  if (key === meter.lastKey) {
    return;
  }
  meter.lastKey = key;
  meter.live.textContent = liveText;
  meter.peak.textContent = `peak ${peakText}`;
  meter.peak.title = `raw contact spike ${rawText}`;
  meter.fill.style.width = width;
  meter.root.dataset['level'] = level;
}
