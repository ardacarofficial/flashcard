/** Focus helpers shared by Dialog and Menu. */

const FOCUSABLE = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled]):not([type="hidden"])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
  '[contenteditable="true"]',
].join(',');

/** Keyboard-focusable elements inside `root`, in DOM order (hidden ones excluded). */
export function focusables(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) => !el.hasAttribute('inert') && !el.closest('[inert]') && el.getAttribute('aria-hidden') !== 'true',
  );
}

/**
 * Keeps Tab / Shift+Tab inside `root`. Returns the keydown handler to attach to `root`.
 */
export function trapTab(root: HTMLElement, e: KeyboardEvent): void {
  if (e.key !== 'Tab') return;
  const list = focusables(root);
  if (list.length === 0) {
    e.preventDefault();
    root.focus();
    return;
  }
  const first = list[0]!;
  const last = list[list.length - 1]!;
  const active = document.activeElement;
  if (e.shiftKey && (active === first || !root.contains(active))) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && (active === last || !root.contains(active))) {
    e.preventDefault();
    first.focus();
  }
}

/** Moves `node` to `document.body` (overlays escape clipping/stacking of their parents). */
export function portal(node: HTMLElement): { destroy: () => void } {
  document.body.appendChild(node);
  return { destroy: () => node.remove() };
}
