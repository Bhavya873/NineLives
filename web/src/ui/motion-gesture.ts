// Turns phone rotation rates into stunt requests, with no page code so it runs in plain Node.
// Twist the phone like a steering wheel to roll; flick the top edge of the landscape screen
// away from you for a front flip, toward you for a back flip; a hard shake is a front flip.
import { StuntCode, type StuntCodeValue } from '../game/stunt-requests.ts';

/** A twist about the screen's normal counts from this rate, in deg/s. */
export const TWIST_RATE_DEG_S = 220;
/** A flick about the screen's horizontal axis counts from this rate, in deg/s. */
export const FLICK_RATE_DEG_S = 260;
/** The winning axis must be at least this many times each of the others. */
export const DOMINANCE_RATIO = 1.5;
/** A shake with no clear axis counts from this total rate, in deg/s. */
export const SHAKE_RATE_DEG_S = 450;
/** After a trigger, further gestures are ignored for this long, in ms. */
export const GESTURE_COOLDOWN_MS = 600;

/** The `rotationRate` of a devicemotion event, in deg/s (null fields count as 0). */
export interface RotationRate {
  alpha: number | null;
  beta: number | null;
  gamma: number | null;
}

/** Rates about the screen's own axes, in deg/s, right-handed. */
export interface ScreenRates {
  /** About the axis out of the screen toward you: positive is counter-clockwise as you see it. */
  twist: number;
  /** About the screen's right axis: positive tips the top edge toward you. */
  pitch: number;
  /** About the screen's up axis (the two sides swinging back and forth). */
  side: number;
}

/**
 * Remaps the device's rotation rates to the screen's axes for `angleDeg`, the value of
 * screen.orientation.angle (0, 90, 180 or 270).
 *
 * Device-axis convention relied on (W3C DeviceMotion, natural portrait): x points to the right
 * edge, y to the top edge, z out of the screen toward you; beta is the rate about x, gamma
 * about y, alpha about z, each counter-clockwise looking down the axis toward the origin
 * (right-hand rule). At angle 90 the phone is turned counter-clockwise, so the screen's right
 * is device -y and its up is device +x; at 270 it is the reverse. z never changes. Browsers
 * have disagreed on these signs in the past; this is untested on real hardware.
 */
export function screenRates(rate: RotationRate, angleDeg: number): ScreenRates {
  const alpha = rate.alpha ?? 0;
  const beta = rate.beta ?? 0;
  const gamma = rate.gamma ?? 0;
  const angle = ((Math.round(angleDeg / 90) % 4) + 4) % 4;
  if (angle === 1) {
    return { twist: alpha, pitch: -gamma, side: beta };
  }
  if (angle === 3) {
    return { twist: alpha, pitch: gamma, side: -beta };
  }
  if (angle === 2) {
    return { twist: alpha, pitch: -beta, side: -gamma };
  }
  return { twist: alpha, pitch: beta, side: gamma };
}

/** True when `value` is big enough on its own and beats both other axes by the ratio. */
function dominates(value: number, thresholdDegS: number, others: number[]): boolean {
  const size = Math.abs(value);
  return (
    size >= thresholdDegS && others.every((other) => size >= DOMINANCE_RATIO * Math.abs(other))
  );
}

/**
 * The stunt one reading asks for, or null. Twisting counter-clockwise rolls left and clockwise
 * rolls right (the way a steering wheel turns); the top edge going away is the front flip.
 */
export function classifyRates(rates: ScreenRates): StuntCodeValue | null {
  if (dominates(rates.twist, TWIST_RATE_DEG_S, [rates.pitch, rates.side])) {
    return rates.twist > 0 ? StuntCode.BarrelLeft : StuntCode.BarrelRight;
  }
  if (dominates(rates.pitch, FLICK_RATE_DEG_S, [rates.twist, rates.side])) {
    return rates.pitch < 0 ? StuntCode.FrontFlip : StuntCode.BackFlip;
  }
  const total = Math.hypot(rates.twist, rates.pitch, rates.side);
  return total >= SHAKE_RATE_DEG_S ? StuntCode.FrontFlip : null;
}

/** When the last gesture fired, so the next one waits out the cooldown. */
export interface GestureGate {
  lastTriggerMs: number;
}

/** A gate that has never fired. */
export function newGestureGate(): GestureGate {
  return { lastTriggerMs: -Infinity };
}

/** One devicemotion reading in, a stunt (or null) out; a trigger starts the cooldown. */
export function detectGesture(
  gate: GestureGate,
  rate: RotationRate,
  angleDeg: number,
  nowMs: number,
): StuntCodeValue | null {
  if (nowMs - gate.lastTriggerMs < GESTURE_COOLDOWN_MS) {
    return null;
  }
  const code = classifyRates(screenRates(rate, angleDeg));
  if (code !== null) {
    gate.lastTriggerMs = nowMs;
  }
  return code;
}
