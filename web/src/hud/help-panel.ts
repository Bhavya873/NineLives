// The keys panel (H): the full key list, shown for a few seconds when play first starts
// and toggled with H after that, so the HUD itself stays clear.

/** How long the panel stays up when play first starts, in ms. */
const SHOW_AT_START_MS = 6000;
/** How long the fade-out takes, in ms; it matches the CSS transition. */
const FADE_MS = 600;

/** The panel and its pending fade, if any. */
export interface HelpPanel {
  element: HTMLElement;
  timer: number | null;
  shownOnce: boolean;
}

/** Wraps the panel element (it starts hidden). */
export function createHelpPanel(element: HTMLElement): HelpPanel {
  element.hidden = true;
  return { element, timer: null, shownOnce: false };
}

/** Shows the panel, then fades it out; only the first time play starts. */
export function showHelpOnFirstPlay(panel: HelpPanel, reducedMotion: boolean): void {
  if (panel.shownOnce) {
    return;
  }
  panel.shownOnce = true;
  showPanel(panel);
  panel.timer = window.setTimeout(() => fadeOut(panel, reducedMotion), SHOW_AT_START_MS);
}

/** H: shows the panel if hidden (it stays), hides it if shown. */
export function toggleHelp(panel: HelpPanel): void {
  panel.shownOnce = true;
  if (panel.element.hidden) {
    showPanel(panel);
  } else {
    clearTimer(panel);
    panel.element.hidden = true;
  }
}

/** Shows the panel at full opacity and cancels any pending fade. */
function showPanel(panel: HelpPanel): void {
  clearTimer(panel);
  panel.element.classList.remove('fading');
  panel.element.hidden = false;
}

/** Fades the panel out, then hides it; with reduced motion it just hides. */
function fadeOut(panel: HelpPanel, reducedMotion: boolean): void {
  if (reducedMotion) {
    panel.element.hidden = true;
    panel.timer = null;
    return;
  }
  panel.element.classList.add('fading');
  panel.timer = window.setTimeout(() => {
    panel.element.hidden = true;
    panel.element.classList.remove('fading');
    panel.timer = null;
  }, FADE_MS);
}

/** Cancels a pending show or fade. */
function clearTimer(panel: HelpPanel): void {
  if (panel.timer !== null) {
    window.clearTimeout(panel.timer);
    panel.timer = null;
  }
}
