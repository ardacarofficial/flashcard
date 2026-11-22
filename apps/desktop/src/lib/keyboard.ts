/**
 * Keyboard rules shared by every global key handler (v1.3, journey J7): keys typed into a text
 * field belong to the field. A window-level shortcut never runs from an `<input>`, `<textarea>`,
 * `<select>` or contenteditable element — Ctrl+A selects the text, Ctrl+Z undoes the typing,
 * letters type — except keys meant for the focused form itself (Ctrl+Enter save, Esc), which
 * that form handles on its own element.
 */

/** `<input>` types that do not edit text: keys on them are free for shortcuts. */
const NON_TEXT_INPUTS = new Set(['button', 'checkbox', 'color', 'file', 'hidden', 'image', 'radio', 'range', 'reset', 'submit']);

/** Whether a key event's target edits text (a text input, textarea, select or contenteditable). */
export function isEditableTarget(target: EventTarget | null): boolean {
  if (typeof Element === 'undefined' || !(target instanceof Element)) return false;
  if (target instanceof HTMLInputElement) return !NON_TEXT_INPUTS.has(target.type);
  if (target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return true;
  if ((target as HTMLElement).isContentEditable) return true;
  // jsdom has no `isContentEditable`: the nearest `contenteditable` attribute decides.
  const owner = target.closest('[contenteditable]');
  return owner !== null && owner.getAttribute('contenteditable') !== 'false';
}

/** Keys that edit or move in text when pressed with Ctrl/⌘ (select all, copy, undo, word jumps…). */
const EDITING_KEYS = new Set(['A', 'C', 'V', 'X', 'Y', 'Z', 'Backspace', 'Delete', 'ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End', 'Enter', 'Tab', 'Space', 'Escape']);

/**
 * Whether a combo (backend form, `Ctrl+Shift+Z`) means something to a text field: every key
 * without Ctrl/⌘/Alt types, and Ctrl/⌘ with an editing key edits.
 */
export function textEditingCombo(combo: string): boolean {
  const parts = combo.split('+');
  const key = parts.at(-1) ?? '';
  const mods = parts.slice(0, -1);
  if (!mods.some((m) => m === 'Ctrl' || m === 'Meta' || m === 'Alt')) return true;
  return EDITING_KEYS.has(key);
}
