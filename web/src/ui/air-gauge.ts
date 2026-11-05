// The HUD's air gauge (SPEC §11.5): a bar showing the air left, "∞" in Arcade, and a red
// flash below 15%. Plain DOM; it reads the air fraction from the game and never
// touches physics.
import { setText } from '../shared/dom.ts';

/** Below this fraction of the tank the gauge flashes red (SPEC §11.5). */
const LOW_AIR_FRACTION = 0.15;

/** The gauge's elements. */
interface AirGauge {
  /** The outer box; gets the `low` class to flash red. */
  box: HTMLElement;
  /** The coloured bar; its width is the air left. */
  fill: HTMLElement;
  /** The text next to the bar: a percentage, or ∞. */
  label: HTMLElement;
}

/** Wraps the gauge elements already in the page. */
export function createAirGauge(box: HTMLElement, fill: HTMLElement, label: HTMLElement): AirGauge {
  return { box, fill, label };
}

/** What the gauge says: ∞ with unlimited air, otherwise the whole percent left. */
export function airGaugeText(airFraction: number, unlimitedAir: boolean): string {
  return unlimitedAir ? '∞' : `${Math.round(airFraction * 100)}%`;
}

/** Shows `airFraction` (0..1) of the tank; a full bar and ∞ when air is unlimited. */
export function updateAirGauge(gauge: AirGauge, airFraction: number, unlimitedAir: boolean): void {
  setText(gauge.label, airGaugeText(airFraction, unlimitedAir));
  const fillFraction = unlimitedAir ? 1 : airFraction;
  const width = `${(fillFraction * 100).toFixed(1)}%`;
  if (gauge.fill.style.width !== width) {
    gauge.fill.style.width = width;
  }
  // Checked every frame, so the flash starts the moment air drops below 15%.
  gauge.box.classList.toggle('low', !unlimitedAir && airFraction < LOW_AIR_FRACTION);
}
