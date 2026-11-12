// The wind readout (HUD only): a small arrow showing which way the wind blows as seen
// from the camera, and a label such as "Breeze 6 m/s". It takes the wind as a world
// vector (the way the air moves, in m/s, +Y up; the sim's `HudField.WindXMS/WindZMS`)
// and the camera's yaw. With no wind it still shows, as "Off" without the arrow, because a
// tap on it (or the V key) cycles the wind.

import { setAriaLabel, setText } from '../shared/dom.ts';

/** Below this speed (m/s) the wind counts as off and the readout hides. */
const WIND_OFF_BELOW_M_S = 0.5;
/** From this speed (m/s) up the wind is a storm rather than a breeze. */
const STORM_FROM_M_S = 12;

/**
 * The camera's yaw in radians from the direction it looks along (any length, world
 * frame): 0 when looking along world +X, growing counter-clockwise seen from above
 * (toward −Z), the right-handed turn about +Y.
 */
export function cameraYawRad(lookX: number, lookZ: number): number {
  return Math.atan2(-lookZ, lookX);
}

/**
 * Screen angle of the wind arrow in radians, clockwise from straight up: 0 means the
 * wind blows away from the camera (into the screen), π/2 to the right, π toward the
 * camera. Only the wind's level part counts; the up/down part is ignored.
 */
export function windArrowAngleRad(windX: number, windZ: number, cameraYaw: number): number {
  // The camera's forward and right directions on the ground (see `cameraYawRad`).
  const forwardX = Math.cos(cameraYaw);
  const forwardZ = -Math.sin(cameraYaw);
  const rightX = Math.sin(cameraYaw);
  const rightZ = Math.cos(cameraYaw);
  const along = windX * forwardX + windZ * forwardZ;
  const across = windX * rightX + windZ * rightZ;
  return Math.atan2(across, along);
}

/** The wind's level for a speed in m/s: "Off", "Breeze" or "Storm". */
export function windKind(speedM_S: number): 'Off' | 'Breeze' | 'Storm' {
  if (speedM_S < WIND_OFF_BELOW_M_S) {
    return 'Off';
  }
  return speedM_S >= STORM_FROM_M_S ? 'Storm' : 'Breeze';
}

/** The label for a level wind speed in m/s: "Off", or "Breeze 6 m/s" / "Storm 20 m/s". */
export function windLabel(speedM_S: number): string {
  const kind = windKind(speedM_S);
  if (kind === 'Off') {
    return kind;
  }
  return `${kind} ${speedM_S.toFixed(0)} m/s`;
}

/** The readout's page elements. */
export interface WindIndicator {
  box: HTMLElement;
  arrow: HTMLElement;
  label: HTMLElement;
}

/** Wraps the readout's elements, found by the caller. */
export function createWindIndicator(
  box: HTMLElement,
  arrow: HTMLElement,
  label: HTMLElement,
): WindIndicator {
  return { box, arrow, label };
}

/**
 * Shows the wind `windWorld` ([x, y, z] in m/s) for a camera at yaw `cameraYaw`;
 * while the wind is off the readout says "Off" and hides only the arrow.
 */
export function updateWindIndicator(
  indicator: WindIndicator,
  windWorld: [number, number, number],
  cameraYaw: number,
): void {
  const [windX, , windZ] = windWorld;
  const speedM_S = Math.hypot(windX, windZ);
  const off = speedM_S < WIND_OFF_BELOW_M_S;
  indicator.arrow.hidden = off;
  setAriaLabel(indicator.box, `Wind: ${windKind(speedM_S)}`);
  if (off) {
    setText(indicator.label, windLabel(0));
    return;
  }
  const angleDeg = (windArrowAngleRad(windX, windZ, cameraYaw) * 180) / Math.PI;
  indicator.arrow.style.transform = `rotate(${angleDeg.toFixed(1)}deg)`;
  setText(indicator.label, windLabel(speedM_S));
}
