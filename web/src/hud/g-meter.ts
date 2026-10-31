// The g-meter (docs/contracts/g-meter.md): a small gauge showing the live passenger g
// (HudField GLoad, already smoothed over 0.1 s by the sim) and the raw peak g of the
// current stunt window (HudField PeakGLoad). It replaces the old screen blackout and
// never darkens the view. The maths is pure so tests run without a browser.

/** The scale's top, in g; the needle pins here. */
export const G_METER_MAX_G = 8;
/** From this g the readout is amber; it matches the reward's soft limit (3 g). */
export const G_METER_WARN_G = 3;
/** From this g the readout is red. */
export const G_METER_HIGH_G = 6;

export type GLevel = 'calm' | 'warn' | 'high';

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

/** Draws live g and the raw peak; skips the DOM when nothing changed. */
export function updateGMeter(meter: GMeter, liveG: number, peakG: number): void {
  const liveText = formatG(liveG);
  const peakText = formatG(peakG);
  const level = gLevel(liveG);
  const width = `${(gNeedleFraction(liveG) * 100).toFixed(0)}%`;
  const key = `${liveText}|${peakText}|${level}|${width}`;
  if (key === meter.lastKey) {
    return;
  }
  meter.lastKey = key;
  meter.live.textContent = liveText;
  meter.peak.textContent = `peak ${peakText}`;
  meter.fill.style.width = width;
  meter.root.dataset['level'] = level;
}
