import { afterEach, describe, expect, it } from 'vitest';
import { isEditableTarget, textEditingCombo } from './keyboard';

afterEach(() => document.body.replaceChildren());

const add = <T extends HTMLElement>(el: T): T => document.body.appendChild(el);

describe('isEditableTarget', () => {
  it('text inputs, textareas and selects edit text', () => {
    for (const type of ['text', 'search', 'email', 'number', 'password', 'url', 'tel', 'date']) {
      const input = add(document.createElement('input'));
      input.type = type;
      expect(isEditableTarget(input), type).toBe(true);
    }
    expect(isEditableTarget(add(document.createElement('textarea')))).toBe(true);
    expect(isEditableTarget(add(document.createElement('select')))).toBe(true);
  });

  it('checkboxes, radios and buttons do not (their keys stay free for shortcuts)', () => {
    for (const type of ['checkbox', 'radio', 'button', 'submit', 'range', 'file']) {
      const input = add(document.createElement('input'));
      input.type = type;
      expect(isEditableTarget(input), type).toBe(false);
    }
    expect(isEditableTarget(add(document.createElement('button')))).toBe(false);
    expect(isEditableTarget(add(document.createElement('div')))).toBe(false);
    expect(isEditableTarget(document.body)).toBe(false);
    expect(isEditableTarget(window)).toBe(false);
    expect(isEditableTarget(null)).toBe(false);
  });

  it('contenteditable elements and their children edit text; contenteditable="false" does not', () => {
    const editor = add(document.createElement('div'));
    editor.setAttribute('contenteditable', 'true');
    const inner = editor.appendChild(document.createElement('span'));
    const locked = editor.appendChild(document.createElement('span'));
    locked.setAttribute('contenteditable', 'false');
    const plain = add(document.createElement('div'));
    plain.setAttribute('contenteditable', '');
    const off = add(document.createElement('div'));
    off.setAttribute('contenteditable', 'false');
    expect(isEditableTarget(editor)).toBe(true);
    expect(isEditableTarget(inner)).toBe(true);
    expect(isEditableTarget(plain)).toBe(true);
    expect(isEditableTarget(locked)).toBe(false);
    expect(isEditableTarget(off)).toBe(false);
  });
});

describe('textEditingCombo', () => {
  it('unmodified keys and Ctrl + editing keys belong to a text field', () => {
    for (const c of ['A', 'N', '2', 'Delete', 'Space', 'Shift+A', 'Ctrl+A', 'Ctrl+Z', 'Ctrl+Shift+Z', 'Meta+V', 'Ctrl+Backspace', 'Ctrl+ArrowLeft']) {
      expect(textEditingCombo(c), c).toBe(true);
    }
  });

  it('other Ctrl / Alt combos do not', () => {
    for (const c of ['Ctrl+K', 'Ctrl+Shift+P', 'Alt+F9', 'Meta+K']) expect(textEditingCombo(c), c).toBe(false);
  });
});
