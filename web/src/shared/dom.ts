// Small helpers for finding elements in the page.

/** Returns the element with this id, or throws a clear error if the page lacks it. */
export function requireElement(id: string): HTMLElement {
  const element = document.getElementById(id);
  if (!element) {
    throw new Error(`The page has no element with id "${id}"`);
  }
  return element;
}

/** True if the player asked the system for less motion (SPEC §11.4). */
export function prefersReducedMotion(): boolean {
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/** Writes `text` only when it changed, so an unchanged HUD costs the page no layout work. */
export function setText(element: HTMLElement, text: string): void {
  if (element.textContent !== text) {
    element.textContent = text;
  }
}

/**
 * Makes the rest of the page inert (no Tab, no clicks, hidden from screen readers) while a
 * modal dialog is up. Returns a function that undoes exactly what it changed.
 */
function inertAround(dialog: HTMLElement): () => void {
  const changed: HTMLElement[] = [];
  for (const child of Array.from(document.body.children)) {
    if (
      child instanceof HTMLElement &&
      child !== dialog &&
      !child.inert &&
      child.tagName !== 'SCRIPT'
    ) {
      child.inert = true;
      changed.push(child);
    }
  }
  return () => {
    for (const child of changed) {
      child.inert = false;
    }
  };
}

/**
 * Moves focus into a dialog, makes the rest of the page inert, and returns a function that
 * undoes both and gives focus back when the dialog closes.
 */
export function focusDialog(dialog: HTMLElement, target: HTMLElement = dialog): () => void {
  const before = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const restoreInert = inertAround(dialog);
  target.focus();
  return () => {
    restoreInert();
    if (before && before !== document.body && before.isConnected) {
      before.focus();
    } else if (document.activeElement instanceof HTMLElement) {
      document.activeElement.blur();
    }
  };
}
