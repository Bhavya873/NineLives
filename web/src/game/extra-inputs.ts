// Wires the touch controls, motion stunts, swipe fallback, gamepad and the tappable wind and
// air-mode readouts to the game's actions. main.ts supplies the actions and reads one merged
// input frame, so the game loop never knows which device was used.
import { requireElement } from '../shared/dom.ts';
import { createAirHint, updateAirHint } from '../ui/air-hint.ts';
import { createGamepad, pollGamepad, readGamepad } from '../ui/gamepad.ts';
import { addInputSource } from '../ui/input-merge.ts';
import {
  motionActive,
  motionSupported,
  requestMotionPermission,
  setMotionEnabled,
  startMotionStunts,
} from '../ui/motion-stunts.ts';
import { enterPhoneSession } from '../ui/phone-session.ts';
import { createSwipeStunts } from '../ui/swipe-stunts.ts';
import { createTouchArrows, readArrows, releaseArrows } from '../ui/touch-arrows.ts';
import { createTouchButtons, readButtons, releaseButtons } from '../ui/touch-buttons.ts';
import { portraitOnTouch, startTouchMode, touchModeOn } from '../ui/touch-mode.ts';
import type { StuntCodeValue } from './stunt-requests.ts';

/** What the game offers to the input devices. */
export interface GameActions {
  requestStunt: (code: StuntCodeValue) => void;
  togglePause: () => void;
  /** The X key: a drop test. */
  dropTest: () => void;
  /** The V key. */
  cycleWind: () => void;
  /** The M key. */
  toggleAirMode: () => void;
  isPaused: () => boolean;
  /** True while a run is live and no wheel touches the ground. */
  isAirborne: () => boolean;
}

/** What main.ts calls on the extra inputs. */
export interface ExtraInputs {
  /** The Play button was pressed (a user gesture). */
  onPlay: () => void;
  /** Once per rendered frame: polls the gamepad and updates the jump hint. */
  renderFrame: (nowMs: number) => void;
  /** Lets go of every held touch control (the game paused under a finger). */
  releaseHeld: () => void;
  /** True while the rotate card covers the game. */
  portraitBlocked: () => boolean;
}

/** Makes a readout button act on click, then drops focus so Space (hop) cannot press it again. */
function onTap(id: string, action: () => void): void {
  const button = requireElement(id);
  button.addEventListener('click', () => {
    action();
    button.blur();
  });
}

/** Wires the pause dialog's touch buttons (Resume, Drop test, Motion stunts). */
function wirePauseButtons(
  actions: GameActions,
  motion: ReturnType<typeof startMotionStunts>,
): void {
  onTap('paused-resume', actions.togglePause);
  onTap('paused-drop', () => {
    // Drop test also resumes, so the fall is seen at once.
    actions.togglePause();
    actions.dropTest();
  });
  const toggle = requireElement('paused-motion');
  toggle.hidden = !motionSupported();
  const showState = () => {
    toggle.textContent = motion.enabled ? 'Motion stunts: on' : 'Motion stunts: off';
    toggle.setAttribute('aria-pressed', String(motion.enabled));
  };
  showState();
  toggle.addEventListener('click', () => {
    setMotionEnabled(motion, !motion.enabled);
    showState();
  });
}

/** Starts every non-keyboard input. Call once at start-up, before the title's onPlay can run. */
export function startExtraInputs(actions: GameActions): ExtraInputs {
  startTouchMode();
  const arrows = createTouchArrows(requireElement('touch-arrows'));
  const buttons = createTouchButtons(requireElement('touch-hop'), requireElement('touch-boost'));
  addInputSource((out) => readArrows(arrows, out));
  addInputSource((out) => readButtons(buttons, out));
  createSwipeStunts(requireElement('touch-swipe-zone'), actions.requestStunt);
  onTap('touch-pause', actions.togglePause);
  onTap('hud-wind', actions.cycleWind);
  onTap('hud-mode', actions.toggleAirMode);

  const motion = startMotionStunts({
    isAirborne: () => !actions.isPaused() && actions.isAirborne(),
    onStunt: actions.requestStunt,
  });
  wirePauseButtons(actions, motion);
  const hint = createAirHint(requireElement('hud-hint'));

  const pad = createGamepad({ onStunt: actions.requestStunt, onPause: actions.togglePause });
  addInputSource((out) => readGamepad(pad, out));

  return {
    onPlay: () => {
      if (touchModeOn()) {
        enterPhoneSession();
        void requestMotionPermission(motion);
      }
    },
    renderFrame: (nowMs) => {
      pollGamepad(pad);
      const airborne = !actions.isPaused() && actions.isAirborne();
      updateAirHint(hint, touchModeOn(), airborne, motionActive(motion), nowMs);
    },
    releaseHeld: () => {
      releaseArrows(arrows);
      releaseButtons(buttons);
    },
    portraitBlocked: portraitOnTouch,
  };
}
