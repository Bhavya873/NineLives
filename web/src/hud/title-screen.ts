// The title screen (SPEC §11.6): mode and thruster pickers, "How it works", and Play.
// The pickers apply at once through callbacks (main.ts talks to the sim); Play or any
// game key hides the panel without swallowing the key, and Esc brings it back.

/** A thruster controller the player can pick, by its `Game.set_controller` name. */
export type ControllerName = 'hybrid' | 'pd' | 'ai' | 'off';

/** A driving mode the player can pick, by its `Game.set_mode` name. */
export type ModeName = 'realistic' | 'arcade';

/** What the title screen tells the game. */
export interface TitleCallbacks {
  /** The player picked a controller. */
  chooseController: (name: ControllerName) => void;
  /** The player picked a mode. */
  chooseMode: (name: ModeName) => void;
}

/** The panel and its picker buttons. */
export interface TitleScreen {
  /** The whole panel. */
  element: HTMLElement;
  /** The controller buttons, each with a `data-controller` name. */
  controllerButtons: HTMLButtonElement[];
  /** The mode buttons, each with a `data-mode` name. */
  modeButtons: HTMLButtonElement[];
}

/** Keys that never close the panel: Esc toggles it, F3 is the overlay, Tab moves focus. */
const KEYS_THAT_KEEP_IT_OPEN = new Set(['Escape', 'F3', 'Tab']);

/**
 * Wires up the panel inside `element` (it starts open). Buttons need
 * `data-controller` or `data-mode` attributes and a `#title-play` button.
 */
export function createTitleScreen(
  element: HTMLElement,
  target: Window,
  callbacks: TitleCallbacks,
): TitleScreen {
  const title: TitleScreen = {
    element,
    controllerButtons: Array.from(element.querySelectorAll('button[data-controller]')),
    modeButtons: Array.from(element.querySelectorAll('button[data-mode]')),
  };
  for (const button of title.controllerButtons) {
    button.addEventListener('click', () => {
      callbacks.chooseController(button.dataset.controller as ControllerName);
      markPicked(title.controllerButtons, button);
    });
  }
  for (const button of title.modeButtons) {
    button.addEventListener('click', () => {
      callbacks.chooseMode(button.dataset.mode as ModeName);
      markPicked(title.modeButtons, button);
    });
  }
  element.querySelector('#title-play')?.addEventListener('click', () => hideTitle(title));
  target.addEventListener('keydown', (event) => {
    // Space or Enter on a focused picker button presses it; it does not start play.
    const onPanel = event.target instanceof Node && element.contains(event.target);
    if (event.code === 'Escape') {
      element.hidden = !element.hidden;
    } else if (!onPanel && !KEYS_THAT_KEEP_IT_OPEN.has(event.code)) {
      // Driving starts the game: hide the panel and let the key reach the car.
      hideTitle(title);
    }
  });
  return title;
}

/** Hides the panel. */
export function hideTitle(title: TitleScreen): void {
  title.element.hidden = true;
}

/** Shows which picker values are current (the M and T keys can change them in play). */
export function showPicked(title: TitleScreen, controller: string, mode: string): void {
  for (const button of title.controllerButtons) {
    button.setAttribute('aria-pressed', String(button.dataset.controller === controller));
  }
  for (const button of title.modeButtons) {
    button.setAttribute('aria-pressed', String(button.dataset.mode === mode));
  }
}

/** Greys out the AI and PD + AI buttons when the policy failed to load. */
export function disableAiPicks(title: TitleScreen): void {
  for (const button of title.controllerButtons) {
    if (button.dataset.controller === 'ai' || button.dataset.controller === 'hybrid') {
      button.disabled = true;
      button.title = 'AI unavailable: the policy did not load';
    }
  }
}

/** Marks `picked` as the pressed button of its group. */
function markPicked(group: HTMLButtonElement[], picked: HTMLButtonElement): void {
  for (const button of group) {
    button.setAttribute('aria-pressed', String(button === picked));
  }
}
