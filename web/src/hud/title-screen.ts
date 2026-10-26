// The title screen (SPEC §11.6): thruster, mode, course and wind pickers, "How it works",
// and Play. Pickers apply at once through callbacks; main.ts talks to the sim.
// Play or any game key hides the panel without swallowing the key; Esc brings it back.

import type { WindLevel } from '../screen-effects.ts';

/** A thruster controller the player can pick, by its `Game.set_controller` name. */
export type ControllerName = 'hybrid' | 'pd' | 'ai' | 'off';

/** A driving mode the player can pick, by its `Game.set_mode` name. */
export type ModeName = 'realistic' | 'arcade';

/** A course the player can pick: the open playground or the Gauntlet race. */
export type CourseName = 'playground' | 'gauntlet';

/** What the title screen tells the game. */
export interface TitleCallbacks {
  /** The player picked a controller. */
  chooseController: (name: ControllerName) => void;
  /** The player picked a mode. */
  chooseMode: (name: ModeName) => void;
  /** The player picked a course. */
  chooseCourse: (name: CourseName) => void;
  /** The player picked a wind level. */
  chooseWind: (level: WindLevel) => void;
  /** The panel closed and play goes on. */
  onHide: () => void;
}

/** The current picks, to mark on the panel. */
export interface TitlePicks {
  controller: string;
  mode: string;
  course: CourseName;
  wind: WindLevel;
}

/** The panel and its picker buttons, one group per `data-` attribute. */
export interface TitleScreen {
  element: HTMLElement;
  groups: Record<keyof TitlePicks, HTMLButtonElement[]>;
  onHide: () => void;
}

/** Keys that never close the panel: Esc toggles it, F3 is the overlay, Tab moves focus. */
const KEYS_THAT_KEEP_IT_OPEN = new Set(['Escape', 'F3', 'Tab']);

/** Wires up the panel inside `element` (it starts open). */
export function createTitleScreen(
  element: HTMLElement,
  target: Window,
  callbacks: TitleCallbacks,
): TitleScreen {
  const title: TitleScreen = {
    element,
    groups: {
      controller: findButtons(element, 'controller'),
      mode: findButtons(element, 'mode'),
      course: findButtons(element, 'course'),
      wind: findButtons(element, 'wind'),
    },
    onHide: callbacks.onHide,
  };
  onPick(title.groups.controller, 'controller', (value) =>
    callbacks.chooseController(value as ControllerName),
  );
  onPick(title.groups.mode, 'mode', (value) => callbacks.chooseMode(value as ModeName));
  onPick(title.groups.course, 'course', (value) => callbacks.chooseCourse(value as CourseName));
  onPick(title.groups.wind, 'wind', (value) => callbacks.chooseWind(value as WindLevel));
  element.querySelector('#title-play')?.addEventListener('click', () => hideTitle(title));
  target.addEventListener('keydown', (event) => {
    // Space or Enter on a focused picker button presses it; it does not start play.
    const onPanel = event.target instanceof Node && element.contains(event.target);
    if (event.code === 'Escape') {
      toggleTitle(title);
    } else if (!onPanel && !KEYS_THAT_KEEP_IT_OPEN.has(event.code)) {
      // Driving starts the game: hide the panel and let the key reach the car.
      hideTitle(title);
    }
  });
  return title;
}

/** Hides the panel. */
export function hideTitle(title: TitleScreen): void {
  if (!title.element.hidden) {
    title.element.hidden = true;
    title.onHide();
  }
}

/** Marks the current picks (the M, T, V, Enter and F keys can change them in play). */
export function showPicked(title: TitleScreen, picks: TitlePicks): void {
  for (const key of Object.keys(title.groups) as (keyof TitlePicks)[]) {
    for (const button of title.groups[key]) {
      button.setAttribute('aria-pressed', String(button.dataset[key] === picks[key]));
    }
  }
}

/** Greys out the AI and PD + AI buttons when the policy failed to load. */
export function disableAiPicks(title: TitleScreen): void {
  for (const button of title.groups.controller) {
    if (button.dataset.controller === 'ai' || button.dataset.controller === 'hybrid') {
      button.disabled = true;
      button.title = 'AI unavailable: the policy did not load';
    }
  }
}

/** Opens a closed panel, or closes an open one. */
function toggleTitle(title: TitleScreen): void {
  if (title.element.hidden) {
    title.element.hidden = false;
  } else {
    hideTitle(title);
  }
}

/** The buttons with a `data-<key>` attribute inside `element`. */
function findButtons(element: HTMLElement, key: string): HTMLButtonElement[] {
  return Array.from(element.querySelectorAll(`button[data-${key}]`));
}

/** Calls `choose` with the button's `data-<key>` value and marks it pressed. */
function onPick(group: HTMLButtonElement[], key: string, choose: (value: string) => void): void {
  for (const button of group) {
    button.addEventListener('click', () => {
      choose(button.dataset[key] ?? '');
      for (const other of group) {
        other.setAttribute('aria-pressed', String(other === button));
      }
    });
  }
}
