// Motion stunts: listens to the phone's rotation sensor and asks for a stunt when you twist or
// flick it in the air (rules in motion-gesture.ts). The on/off choice is remembered in
// localStorage. If motion is missing, denied or off, the swipe fallback is the stunt input.
import type { StuntCodeValue } from '../game/stunt-requests.ts';
import { detectGesture, newGestureGate, type RotationRate } from './motion-gesture.ts';

const SETTING_KEY = 'ninelives.motionStunts';

/** The motion state the page shares. */
export interface MotionStunts {
  /** The player's on/off choice (default on). */
  enabled: boolean;
  /** False after the player said no to the iOS permission prompt. */
  permitted: boolean;
  /** True once the browser has really sent rotation readings. */
  sawReading: boolean;
}

/** iOS adds a permission method to DeviceMotionEvent; other browsers do not have it. */
interface PermissionedDeviceMotion {
  requestPermission?: () => Promise<'granted' | 'denied'>;
}

function readSetting(): boolean {
  try {
    return window.localStorage.getItem(SETTING_KEY) !== 'off';
  } catch {
    return true;
  }
}

/** Remembers the on/off choice; a blocked localStorage just forgets it. */
function saveSetting(enabled: boolean): void {
  try {
    window.localStorage.setItem(SETTING_KEY, enabled ? 'on' : 'off');
  } catch {
    // private window or blocked site data: the choice lasts until the page closes
  }
}

/** True when this browser can send motion readings at all. */
export function motionSupported(): boolean {
  return typeof DeviceMotionEvent !== 'undefined';
}

/** Motion stunts are usable now: on, allowed, and the sensor has been heard from. */
export function motionActive(motion: MotionStunts): boolean {
  return motion.enabled && motion.permitted && motion.sawReading;
}

/** Sets and remembers the player's on/off choice. */
export function setMotionEnabled(motion: MotionStunts, enabled: boolean): void {
  motion.enabled = enabled;
  saveSetting(enabled);
}

/**
 * Asks iOS for motion permission. Must run inside a tap or click (the Play button). Browsers
 * without the permission method need none.
 */
export async function requestMotionPermission(motion: MotionStunts): Promise<void> {
  const permissioned = (
    typeof DeviceMotionEvent === 'undefined' ? undefined : DeviceMotionEvent
  ) as PermissionedDeviceMotion | undefined;
  if (!permissioned?.requestPermission) {
    return;
  }
  try {
    motion.permitted = (await permissioned.requestPermission()) === 'granted';
  } catch {
    motion.permitted = false;
  }
}

/** What the page gives the motion listener. */
interface MotionHooks {
  /** True while the car is in the air and a run is live. */
  isAirborne: () => boolean;
  onStunt: (code: StuntCodeValue) => void;
}

/** Starts listening. Gestures count only in the air; the car's planner still decides. */
export function startMotionStunts(hooks: MotionHooks): MotionStunts {
  const motion: MotionStunts = { enabled: readSetting(), permitted: true, sawReading: false };
  const gate = newGestureGate();
  window.addEventListener('devicemotion', (event) => {
    const rate: RotationRate | null = event.rotationRate;
    if (!rate || rate.alpha === null) {
      return;
    }
    motion.sawReading = true;
    if (!motion.enabled || !motion.permitted || !hooks.isAirborne()) {
      return;
    }
    const code = detectGesture(
      gate,
      rate,
      window.screen.orientation?.angle ?? 90,
      performance.now(),
    );
    if (code !== null) {
      hooks.onStunt(code);
    }
  });
  return motion;
}
