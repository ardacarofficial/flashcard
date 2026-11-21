/**
 * Page-level keys of Home and the deck page (v1.2): plain `Enter` starts studying when the
 * focus is not on something that handles Enter itself (a field, a button, a link, the preview
 * card) and no dialog or menu is open.
 */
const INTERACTIVE = 'input, textarea, select, button, a[href], [contenteditable], [role=button], [role=menuitem], [role=option], [role=tab], [tabindex]:not([tabindex="-1"])';

/** Whether `e` is a bare Enter meant for the page (not for a control or an open overlay). */
export function pageEnter(e: KeyboardEvent): boolean {
  if (e.key !== 'Enter' || e.defaultPrevented || e.repeat) return false;
  if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey || e.isComposing) return false;
  const target = e.target;
  if (target instanceof HTMLElement && (target.isContentEditable || target.closest(INTERACTIVE))) return false;
  if (document.querySelector('[role=dialog], [role=alertdialog], [role=menu]')) return false;
  return true;
}
