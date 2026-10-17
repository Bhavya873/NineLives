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
