// The title screen: Play and a link to the training viewer. The game's clock
// waits behind it. `?play` (any value) skips it, which the tests and shared links use.
import { focusDialog } from '../shared/dom.ts';

/** True when the page address does not ask to skip the title. */
export function titleWanted(params: URLSearchParams): boolean {
  return !params.has('play');
}

/** The title's elements and whether it is up. */
interface Title {
  root: HTMLElement;
  visible: boolean;
}

/**
 * Wires the title. `onPlay` runs once when the player starts. Enter or Space also starts it
 * while the title is up. With `wanted` false the title is hidden at once and `onPlay` runs.
 */
export function createTitle(
  root: HTMLElement,
  playButton: HTMLElement,
  wanted: boolean,
  onPlay: () => void,
): Title {
  const title: Title = { root, visible: wanted };
  root.hidden = !wanted;
  // The title is a dialog: focus starts on Play and goes back where it was when it closes.
  const restoreFocus = wanted ? focusDialog(root, playButton) : () => undefined;
  const play = () => {
    if (!title.visible) {
      return;
    }
    title.visible = false;
    root.hidden = true;
    restoreFocus();
    onPlay();
  };
  playButton.addEventListener('click', play);
  window.addEventListener('keydown', (event) => {
    if (title.visible && (event.code === 'Enter' || event.code === 'Space')) {
      // a key on the watch link belongs to the link
      if (event.target instanceof HTMLAnchorElement) {
        return;
      }
      event.preventDefault();
      play();
    }
  });
  if (!wanted) {
    onPlay();
  }
  return title;
}
