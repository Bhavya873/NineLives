// Switches on the glass refraction (SPEC §11.5) where the browser can draw it: Chromium renders
// an SVG filter inside backdrop-filter. Elsewhere the blur-only glass in theme.css shows.

/** True when backdrop-filter accepts an SVG filter AND the engine is Chromium. */
export function supportsRefraction(): boolean {
  if (!window.CSS || !CSS.supports) {
    return false;
  }
  const parses =
    CSS.supports('backdrop-filter', 'url(#glass-refract) blur(3px)') ||
    CSS.supports('-webkit-backdrop-filter', 'url(#glass-refract) blur(3px)');
  if (!parses) {
    return false;
  }
  // Other engines may parse url() here but not draw it, so also ask for a Chromium brand.
  const nav = navigator as Navigator & { userAgentData?: { brands: { brand: string }[] } };
  const brands = nav.userAgentData?.brands ?? [];
  return brands.some((entry) => /Chromium/i.test(entry.brand));
}

/** Sets `data-refract="on"` on <html> when refraction is supported. Call once at start-up. */
export function enableRefractionIfSupported(): void {
  document.documentElement.dataset['refract'] = supportsRefraction() ? 'on' : 'off';
}
